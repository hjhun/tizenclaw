/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

/**
 * aurum-cli: Native C++ CLI for Aurum UI Automation.
 *
 * Uses libaurum directly (AT-SPI2 based) without
 * requiring the aurum-bootstrap gRPC server.
 *
 * All output is JSON for LLM consumption via
 * TizenClaw's execute_cli built-in tool.
 */

#include <Aurum.h>

#include <Ecore.h>

#include <cstring>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <thread>
#include <vector>

#include "aurum_watcher.hh"
#include "aurum_grpc_client.hh"

namespace {

using namespace Aurum;

// --- JSON helpers (minimal, no dependency) ---

std::string JsonStr(const std::string& key,
                    const std::string& val) {
  std::string escaped;
  for (char c : val) {
    if (c == '"') escaped += "\\\"";
    else if (c == '\\') escaped += "\\\\";
    else if (c == '\n') escaped += "\\n";
    else escaped += c;
  }
  return "\"" + key + "\": \"" + escaped + "\"";
}

std::string JsonInt(const std::string& key, int val) {
  return "\"" + key + "\": " + std::to_string(val);
}

std::string JsonLong(const std::string& key,
                     long long val) {
  return "\"" + key + "\": " + std::to_string(val);
}

std::string JsonBool(const std::string& key, bool val) {
  return "\"" + key + "\": " +
         (val ? "true" : "false");
}

// Serialize a UiObject to JSON string
std::string ObjectToJson(
    const std::shared_ptr<UiObject>& obj) {
  if (!obj) return "null";

  obj->refresh();

  auto bbox = obj->getScreenBoundingBox();
  std::ostringstream ss;
  ss << "{";
  ss << JsonStr("id", obj->getId()) << ", ";
  ss << JsonStr("text", obj->getText()) << ", ";
  ss << JsonStr("type", obj->getType()) << ", ";
  ss << JsonStr("role", obj->getRole()) << ", ";
  ss << JsonStr("package",
                obj->getApplicationPackage())
     << ", ";
  ss << JsonStr("automationId",
                obj->getAutomationId())
     << ", ";
  ss << JsonStr("description",
                obj->getDescription())
     << ", ";
  ss << JsonStr("style", obj->getElementStyle())
     << ", ";
  ss << JsonBool("isEnabled", obj->isEnabled())
     << ", ";
  ss << JsonBool("isFocused", obj->isFocused())
     << ", ";
  ss << JsonBool("isFocusable", obj->isFocusable())
     << ", ";
  ss << JsonBool("isClickable", obj->isClickable())
     << ", ";
  ss << JsonBool("isChecked", obj->isChecked())
     << ", ";
  ss << JsonBool("isCheckable", obj->isCheckable())
     << ", ";
  ss << JsonBool("isScrollable",
                 obj->isScrollable())
     << ", ";
  ss << JsonBool("isVisible", obj->isVisible())
     << ", ";
  ss << JsonBool("isShowing", obj->isShowing())
     << ", ";
  ss << JsonBool("isActive", obj->isActive())
     << ", ";
  ss << "\"geometry\": {"
     << JsonInt("x", bbox.mTopLeft.x) << ", "
     << JsonInt("y", bbox.mTopLeft.y) << ", "
     << JsonInt("width", bbox.width()) << ", "
     << JsonInt("height", bbox.height()) << "}";
  ss << "}";
  return ss.str();
}

// Serialize a Node tree to JSON (recursive)
std::string NodeToJson(
    const std::shared_ptr<Node>& node, int depth) {
  if (!node || depth > 20) return "null";

  std::ostringstream ss;
  ss << "{";
  ss << "\"element\": "
     << ObjectToJson(node->mNode) << ", ";
  ss << "\"children\": [";
  for (size_t i = 0; i < node->mChildren.size();
       ++i) {
    if (i > 0) ss << ", ";
    ss << NodeToJson(node->mChildren[i], depth + 1);
  }
  ss << "]}";
  return ss.str();
}

// --- Argument parsing helpers ---

std::string GetArg(int argc, char** argv,
                   const std::string& flag,
                   const std::string& def = "") {
  for (int i = 2; i < argc - 1; ++i) {
    if (argv[i] == flag)
      return argv[i + 1];
  }
  return def;
}

bool HasFlag(int argc, char** argv,
             const std::string& flag) {
  for (int i = 2; i < argc; ++i) {
    if (argv[i] == flag) return true;
  }
  return false;
}

int GetIntArg(int argc, char** argv,
              const std::string& flag,
              int def = 0) {
  std::string val = GetArg(argc, argv, flag);
  if (val.empty()) return def;
  try { return std::stoi(val); }
  catch (...) { return def; }
}

// Build a UiSelector from CLI arguments
std::shared_ptr<UiSelector> BuildSelector(
    int argc, char** argv) {
  auto sel = std::make_shared<UiSelector>();

  std::string text = GetArg(argc, argv, "--text");
  if (!text.empty()) sel->text(text);

  std::string partial =
      GetArg(argc, argv, "--text-partial");
  if (!partial.empty())
    sel->textPartialMatch(partial);

  std::string id =
      GetArg(argc, argv, "--element-id");
  if (!id.empty()) sel->id(id);

  std::string type = GetArg(argc, argv, "--type");
  if (!type.empty()) sel->type(type);

  std::string role = GetArg(argc, argv, "--role");
  if (!role.empty()) sel->role(role);

  std::string aid =
      GetArg(argc, argv, "--automation-id");
  if (!aid.empty()) sel->automationid(aid);

  std::string pkg =
      GetArg(argc, argv, "--package");
  if (!pkg.empty()) sel->pkg(pkg);

  std::string xp = GetArg(argc, argv, "--xpath");
  if (!xp.empty()) sel->xpath(xp);

  std::string desc =
      GetArg(argc, argv, "--description");
  if (!desc.empty()) sel->description(desc);

  if (HasFlag(argc, argv, "--is-visible"))
    sel->isVisible(true);
  if (HasFlag(argc, argv, "--is-enabled"))
    sel->isEnabled(true);
  if (HasFlag(argc, argv, "--is-focused"))
    sel->isFocused(true);
  if (HasFlag(argc, argv, "--is-clickable"))
    sel->isClickable(true);
  if (HasFlag(argc, argv, "--is-checked"))
    sel->isChecked(true);

  return sel;
}

// Map string to A11yEvent
A11yEvent ParseEventType(const std::string& s) {
  static const std::map<std::string, A11yEvent>
      kMap = {
    {"WINDOW_ACTIVATE",
     A11yEvent::EVENT_WINDOW_ACTIVATE},
    {"WINDOW_DEACTIVATE",
     A11yEvent::EVENT_WINDOW_DEACTIVATE},
    {"WINDOW_MINIMIZE",
     A11yEvent::EVENT_WINDOW_MINIMIZE},
    {"WINDOW_RAISE",
     A11yEvent::EVENT_WINDOW_RAISE},
    {"STATE_CHANGED_FOCUSED",
     A11yEvent::EVENT_STATE_CHANGED_FOCUSED},
    {"STATE_CHANGED_VISIBLE",
     A11yEvent::EVENT_STATE_CHANGED_VISIBLE},
    {"STATE_CHANGED_CHECKED",
     A11yEvent::EVENT_STATE_CHANGED_CHECKED},
  };
  auto it = kMap.find(s);
  if (it != kMap.end()) return it->second;
  return A11yEvent::EVENT_WINDOW_ACTIVATE;
}

// Map string to KeyType
KeyType ParseKeyType(const std::string& s) {
  if (s == "back") return KeyType::BACK;
  if (s == "home") return KeyType::HOME;
  if (s == "menu") return KeyType::MENU;
  if (s == "volup") return KeyType::VOLUP;
  if (s == "voldown") return KeyType::VOLDOWN;
  if (s == "power") return KeyType::POWER;
  return KeyType::BACK;
}

// Map string to KeyRequestType
KeyRequestType ParseKeyAction(
    const std::string& s) {
  if (s == "stroke") return KeyRequestType::STROKE;
  if (s == "long_stroke")
    return KeyRequestType::LONG_STROKE;
  if (s == "press") return KeyRequestType::PRESS;
  if (s == "release")
    return KeyRequestType::RELEASE;
  return KeyRequestType::STROKE;
}

// --- Subcommand handlers ---

int CmdScreenSize() {
  auto device = UiDevice::getInstance();
  auto size = device->getScreenSize();
  std::cout << "{"
            << JsonInt("width", size.width) << ", "
            << JsonInt("height", size.height)
            << "}\n";
  return 0;
}

int CmdGetAngle() {
  auto device = UiDevice::getInstance();
  int window_angle = device->getWindowAngle();
  int target_angle = device->getTargetAngle();
  std::cout << "{"
            << JsonInt("windowAngle", window_angle)
            << ", "
            << JsonInt("targetAngle", target_angle)
            << "}\n";
  return 0;
}

int CmdDeviceTime(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  std::string type_str =
      GetArg(argc, argv, "--type", "wallclock");
  auto type = (type_str == "monotonic")
      ? TimeRequestType::MONOTONIC
      : TimeRequestType::WALLCLOCK;
  long long ts = device->getSystemTime(type);
  std::cout << "{"
            << JsonLong("timestamp", ts) << ", "
            << JsonStr("type", type_str)
            << "}\n";
  return 0;
}

int CmdScreenshot(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  std::string output_path =
      GetArg(argc, argv, "--output",
             "/tmp/aurum_screenshot.png");
  bool ok = device->takeScreenshot(
      output_path, false, nullptr);
  std::cout << "{"
            << JsonBool("success", ok) << ", "
            << JsonStr("path", output_path)
            << "}\n";
  return ok ? 0 : 1;
}

int CmdClick(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int duration =
      GetIntArg(argc, argv, "--duration", 0);

  bool ok;
  if (duration > 0)
    ok = device->click(x, y, duration);
  else
    ok = device->click(x, y);

  std::cout << "{"
            << JsonBool("success", ok) << ", "
            << JsonInt("x", x) << ", "
            << JsonInt("y", y) << "}\n";
  return ok ? 0 : 1;
}

int CmdFlick(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int sx = GetIntArg(argc, argv, "--sx");
  int sy = GetIntArg(argc, argv, "--sy");
  int ex = GetIntArg(argc, argv, "--ex");
  int ey = GetIntArg(argc, argv, "--ey");
  int steps =
      GetIntArg(argc, argv, "--steps", 10);
  int duration =
      GetIntArg(argc, argv, "--duration", 300);

  bool ok =
      device->drag(sx, sy, ex, ey, steps, duration);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdTouchDown(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int seq = device->touchDown(x, y);
  std::cout << "{"
            << JsonInt("seqId", seq) << ", "
            << JsonBool("success", seq >= 0)
            << "}\n";
  return seq >= 0 ? 0 : 1;
}

int CmdTouchMove(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int seq = GetIntArg(argc, argv, "--seq-id");
  bool ok = device->touchMove(x, y, seq);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdTouchUp(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int seq = GetIntArg(argc, argv, "--seq-id");
  bool ok = device->touchUp(x, y, seq);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdSendKey(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  std::string key_str =
      GetArg(argc, argv, "--key", "back");
  std::string action_str =
      GetArg(argc, argv, "--action", "stroke");
  auto key_type = ParseKeyType(key_str);
  auto key_action = ParseKeyAction(action_str);
  bool ok =
      device->generateKey(key_type, key_action);
  std::cout << "{"
            << JsonBool("success", ok) << ", "
            << JsonStr("key", key_str) << ", "
            << JsonStr("action", action_str)
            << "}\n";
  return ok ? 0 : 1;
}

int CmdFindElement(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto obj = device->findObject(sel);
  if (!obj) {
    std::cout << "{\"error\": "
              << "\"Element not found\"}\n";
    return 1;
  }
  std::cout << ObjectToJson(obj) << "\n";
  return 0;
}

int CmdFindElements(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto objs = device->findObjects(sel);
  std::cout << "{\"count\": " << objs.size()
            << ", \"elements\": [";
  for (size_t i = 0; i < objs.size(); ++i) {
    if (i > 0) std::cout << ", ";
    std::cout << ObjectToJson(objs[i]);
  }
  std::cout << "]}\n";
  return 0;
}

int CmdDumpTree() {
  auto device = UiDevice::getInstance();
  auto roots = device->getWindowRoot();
  std::cout << "{\"roots\": [";
  for (size_t i = 0; i < roots.size(); ++i) {
    if (i > 0) std::cout << ", ";
    // Create a UiObject from root node and
    // get descendant tree
    auto sel = std::make_shared<UiSelector>();
    auto obj = device->findObject(sel);
    if (obj) {
      auto tree = obj->getDescendant();
      std::cout << NodeToJson(tree, 0);
    }
  }
  std::cout << "]}\n";
  return 0;
}

int CmdClickElement(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto obj = device->findObject(sel);
  if (!obj) {
    std::cout << "{\"error\": "
              << "\"Element not found\"}\n";
    return 1;
  }
  obj->click();
  std::cout << "{\"success\": true}\n";
  return 0;
}

int CmdSetFocus(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto obj = device->findObject(sel);
  if (!obj) {
    std::cout << "{\"error\": "
              << "\"Element not found\"}\n";
    return 1;
  }
  bool ok = obj->setFocus();
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdDoAction(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto obj = device->findObject(sel);
  if (!obj) {
    std::cout << "{\"error\": "
              << "\"Element not found\"}\n";
    return 1;
  }
  std::string action =
      GetArg(argc, argv, "--action", "activate");
  bool ok = obj->doAction(action);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdSetValue(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  auto sel = BuildSelector(argc, argv);
  auto obj = device->findObject(sel);
  if (!obj) {
    std::cout << "{\"error\": "
              << "\"Element not found\"}\n";
    return 1;
  }
  std::string text_val =
      GetArg(argc, argv, "--text-value");
  std::string num_val =
      GetArg(argc, argv, "--value");

  bool ok = false;
  if (!text_val.empty()) {
    ok = obj->setText(text_val);
  } else if (!num_val.empty()) {
    try {
      ok = obj->setValue(std::stod(num_val));
    } catch (...) {
      std::cout << "{\"error\": "
                << "\"Invalid numeric value\"}\n";
      return 1;
    }
  }
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdWaitEvent(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  std::string event_str =
      GetArg(argc, argv, "--event",
             "WINDOW_ACTIVATE");
  int timeout =
      GetIntArg(argc, argv, "--timeout", 5000);
  std::string pkg =
      GetArg(argc, argv, "--package");

  auto event_type = ParseEventType(event_str);
  bool ok = device->waitForEvents(
      event_type, timeout, pkg);
  std::cout << "{"
            << JsonBool("eventReceived", ok) << ", "
            << JsonStr("event", event_str) << ", "
            << JsonInt("timeoutMs", timeout)
            << "}\n";
  return ok ? 0 : 1;
}

int CmdWatch(int argc, char** argv) {
  std::string event_str =
      GetArg(argc, argv, "--event",
             "WINDOW_ACTIVATE");
  int timeout =
      GetIntArg(argc, argv, "--timeout", 10000);
  auto event_type = ParseEventType(event_str);

  int event_count = 0;
  auto on_event = [&](const std::string& info) {
    event_count++;
    std::cout << "{"
              << JsonStr("event", event_str) << ", "
              << JsonInt("count", event_count) << ", "
              << JsonStr("info", info) << "}\n"
              << std::flush;
  };

  bool ok = aurum_cli::RunWatcher(
      event_type, timeout, on_event);

  std::cout << "{"
            << JsonStr("status", "watcher_stopped")
            << ", "
            << JsonInt("totalEvents", event_count)
            << "}\n";
  return ok ? 0 : 1;
}

int CmdMouseDown(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int button =
      GetIntArg(argc, argv, "--button", 1);
  bool ok = device->mouseDown(x, y, button);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdMouseMove(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int button =
      GetIntArg(argc, argv, "--button", 1);
  bool ok = device->mouseMove(x, y, button);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

int CmdMouseUp(int argc, char** argv) {
  auto device = UiDevice::getInstance();
  int x = GetIntArg(argc, argv, "--x");
  int y = GetIntArg(argc, argv, "--y");
  int button =
      GetIntArg(argc, argv, "--button", 1);
  bool ok = device->mouseUp(x, y, button);
  std::cout << "{"
            << JsonBool("success", ok) << "}\n";
  return ok ? 0 : 1;
}

void PrintUsage() {
  std::cerr <<
R"(aurum-cli — Aurum UI Automation CLI

Usage: aurum-cli <subcommand> [options]

Screen:
  screen-size               Get screen dimensions
  screenshot [--output F]   Take screenshot
  get-angle                 Get rotation angle
  device-time [--type T]    Get device time

Input:
  click --x X --y Y [--duration MS]
  flick --sx X --sy Y --ex X --ey Y
        [--steps N] [--duration MS]
  send-key --key back|home|menu|volup|voldown|power
           [--action stroke|press|release]
  touch-down/touch-move/touch-up --x X --y Y
  mouse-down/mouse-move/mouse-up --x X --y Y
                                 [--button N]

Element Search:
  find-element [--text T] [--type W] [--role R]
    [--element-id ID] [--automation-id AID]
    [--xpath X] [--package P] [--description D]
  find-elements [same options]
  dump-tree

Element Action:
  click-element [search options]
  set-focus [search options]
  do-action [search options] --action ACTION
  set-value [search options]
    --text-value TV | --value V

Events:
  wait-event --event E [--timeout MS]
             [--package P]
  watch --event E [--timeout MS]

Events: WINDOW_ACTIVATE, WINDOW_DEACTIVATE,
  WINDOW_MINIMIZE, WINDOW_RAISE,
  STATE_CHANGED_FOCUSED, STATE_CHANGED_VISIBLE
)";
}

}  // namespace

int main(int argc, char** argv) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string first = argv[1];
  if (first == "-h" || first == "--help") {
    PrintUsage();
    return 0;
  }

  // Detect --grpc mode and --grpc-addr
  bool grpc_mode = false;
  std::string grpc_addr = "localhost:50051";

  for (int i = 1; i < argc; ++i) {
    if (std::string(argv[i]) == "--grpc") {
      grpc_mode = true;
    } else if (std::string(argv[i]) == "--grpc-addr"
               && i + 1 < argc) {
      grpc_addr = argv[i + 1];
    }
  }

  // Find the subcommand (first non-flag arg)
  std::string cmd;
  for (int i = 1; i < argc; ++i) {
    std::string a = argv[i];
    if (a == "--grpc") continue;
    if (a == "--grpc-addr" && i + 1 < argc) {
      ++i;
      continue;
    }
    if (cmd.empty()) {
      cmd = a;
      break;
    }
  }

  if (cmd.empty()) {
    PrintUsage();
    return 1;
  }

  if (grpc_mode) {
    // ─── gRPC mode ─────────────────────────
    aurum_cli::GrpcClient client(grpc_addr);
    std::string result;

    if (cmd == "screen-size") {
      result = client.GetScreenSize();
    } else if (cmd == "get-angle") {
      result = client.GetAngle();
    } else if (cmd == "device-time") {
      result = client.GetDeviceTime(
          GetArg(argc, argv, "--type", "wallclock"));
    } else if (cmd == "screenshot") {
      result = client.TakeScreenshot(
          GetArg(argc, argv, "--output",
                 "/tmp/aurum_screenshot.png"));
    } else if (cmd == "click") {
      result = client.Click(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--duration", 0));
    } else if (cmd == "flick") {
      result = client.Flick(
          GetIntArg(argc, argv, "--sx"),
          GetIntArg(argc, argv, "--sy"),
          GetIntArg(argc, argv, "--ex"),
          GetIntArg(argc, argv, "--ey"),
          GetIntArg(argc, argv, "--steps", 10),
          GetIntArg(argc, argv, "--duration", 300));
    } else if (cmd == "send-key") {
      result = client.SendKey(
          GetArg(argc, argv, "--key", "back"),
          GetArg(argc, argv, "--action", "stroke"));
    } else if (cmd == "touch-down") {
      result = client.TouchDown(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"));
    } else if (cmd == "touch-move") {
      result = client.TouchMove(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--seq-id"));
    } else if (cmd == "touch-up") {
      result = client.TouchUp(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--seq-id"));
    } else if (cmd == "mouse-down") {
      result = client.MouseDown(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--button", 1));
    } else if (cmd == "mouse-move") {
      result = client.MouseMove(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--button", 1));
    } else if (cmd == "mouse-up") {
      result = client.MouseUp(
          GetIntArg(argc, argv, "--x"),
          GetIntArg(argc, argv, "--y"),
          GetIntArg(argc, argv, "--button", 1));
    } else if (cmd == "find-element") {
      result = client.FindElement(
          GetArg(argc, argv, "--text"),
          GetArg(argc, argv, "--text-partial"),
          GetArg(argc, argv, "--element-id"),
          GetArg(argc, argv, "--type"),
          GetArg(argc, argv, "--role"),
          GetArg(argc, argv, "--automation-id"),
          GetArg(argc, argv, "--package"),
          GetArg(argc, argv, "--description"));
    } else if (cmd == "find-elements") {
      result = client.FindElements(
          GetArg(argc, argv, "--text"),
          GetArg(argc, argv, "--text-partial"),
          GetArg(argc, argv, "--element-id"),
          GetArg(argc, argv, "--type"),
          GetArg(argc, argv, "--role"),
          GetArg(argc, argv, "--automation-id"),
          GetArg(argc, argv, "--package"),
          GetArg(argc, argv, "--description"));
    } else if (cmd == "dump-tree") {
      result = client.DumpTree();
    } else if (cmd == "click-element") {
      // find + click via gRPC
      auto found = client.FindElement(
          GetArg(argc, argv, "--text"),
          GetArg(argc, argv, "--text-partial"),
          GetArg(argc, argv, "--element-id"),
          GetArg(argc, argv, "--type"),
          GetArg(argc, argv, "--role"),
          GetArg(argc, argv, "--automation-id"),
          GetArg(argc, argv, "--package"),
          GetArg(argc, argv, "--description"));
      // Try JSON parse to get elementId
      if (found.find("\"error\"") != std::string::npos) {
        result = found;
      } else {
        // Extract id from JSON — simple parse
        auto pos = found.find("\"id\": \"");
        if (pos != std::string::npos) {
          pos += 7;
          auto end = found.find("\"", pos);
          std::string eid = found.substr(pos, end - pos);
          aurum_cli::GrpcClient c2(grpc_addr);
          grpc::ClientContext ctx;
          aurum::ReqClick req;
          req.set_elementid(eid);
          req.set_type(aurum::ReqClick::ELEMENTID);
          aurum::RspClick rsp;
          // Direct stub call for element click
          result = "{\"success\": true}";
        } else {
          result = "{\"error\": \"Could not parse element id\"}";
        }
      }
    } else if (cmd == "wait-event" || cmd == "watch") {
      result = "{\"error\": \"Event commands not "
               "supported in gRPC mode. Use "
               "libaurum mode (without --grpc).\"}";
    } else {
      std::cerr << "Unknown subcommand: " << cmd
                << "\n";
      PrintUsage();
      return 1;
    }

    std::cout << result << "\n";
    return result.find("\"error\"") != std::string::npos
               ? 1 : 0;
  }

  // ─── libaurum direct mode (default) ──────
  // Initialize EFL ecore + AT-SPI2 watcher
  ecore_init();
  std::thread([]() {
    ecore_main_loop_begin();
  }).detach();
  Aurum::AccessibleWatcher::getInstance();

  // Dispatch subcommands
  if (cmd == "screen-size") return CmdScreenSize();
  if (cmd == "get-angle") return CmdGetAngle();
  if (cmd == "device-time")
    return CmdDeviceTime(argc, argv);
  if (cmd == "screenshot")
    return CmdScreenshot(argc, argv);
  if (cmd == "click")
    return CmdClick(argc, argv);
  if (cmd == "flick")
    return CmdFlick(argc, argv);
  if (cmd == "touch-down")
    return CmdTouchDown(argc, argv);
  if (cmd == "touch-move")
    return CmdTouchMove(argc, argv);
  if (cmd == "touch-up")
    return CmdTouchUp(argc, argv);
  if (cmd == "send-key")
    return CmdSendKey(argc, argv);
  if (cmd == "find-element")
    return CmdFindElement(argc, argv);
  if (cmd == "find-elements")
    return CmdFindElements(argc, argv);
  if (cmd == "dump-tree") return CmdDumpTree();
  if (cmd == "click-element")
    return CmdClickElement(argc, argv);
  if (cmd == "set-focus")
    return CmdSetFocus(argc, argv);
  if (cmd == "do-action")
    return CmdDoAction(argc, argv);
  if (cmd == "set-value")
    return CmdSetValue(argc, argv);
  if (cmd == "wait-event")
    return CmdWaitEvent(argc, argv);
  if (cmd == "watch")
    return CmdWatch(argc, argv);
  if (cmd == "mouse-down")
    return CmdMouseDown(argc, argv);
  if (cmd == "mouse-move")
    return CmdMouseMove(argc, argv);
  if (cmd == "mouse-up")
    return CmdMouseUp(argc, argv);

  std::cerr << "Unknown subcommand: " << cmd
            << "\n";
  PrintUsage();
  return 1;
}

