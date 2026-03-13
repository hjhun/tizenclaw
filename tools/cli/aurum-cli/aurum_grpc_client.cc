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

#include "aurum_grpc_client.hh"

#include <chrono>
#include <sstream>

namespace aurum_cli {

namespace {

std::string JStr(const std::string& k,
                 const std::string& v) {
  std::string e;
  for (char c : v) {
    if (c == '"') e += "\\\"";
    else if (c == '\\') e += "\\\\";
    else if (c == '\n') e += "\\n";
    else e += c;
  }
  return "\"" + k + "\": \"" + e + "\"";
}

std::string JInt(const std::string& k, int v) {
  return "\"" + k + "\": " + std::to_string(v);
}

std::string JLong(const std::string& k,
                  long long v) {
  return "\"" + k + "\": " + std::to_string(v);
}

std::string JBool(const std::string& k, bool v) {
  return "\"" + k + "\": " +
         (v ? "true" : "false");
}

std::string GrpcErr(const grpc::Status& s) {
  return "{\"error\": \"gRPC: " +
         s.error_message() + "\"}";
}

}  // namespace

GrpcClient::GrpcClient(const std::string& addr) {
  auto channel = grpc::CreateChannel(
      addr, grpc::InsecureChannelCredentials());
  // Wait up to 5s for the channel to connect
  channel->WaitForConnected(
      gpr_time_add(gpr_now(GPR_CLOCK_REALTIME),
                   gpr_time_from_seconds(5, GPR_TIMESPAN)));
  stub_ = aurum::Bootstrap::NewStub(channel);
}

std::unique_ptr<grpc::ClientContext>
GrpcClient::MakeCtx(int timeout_sec) {
  auto ctx = std::make_unique<grpc::ClientContext>();
  ctx->set_deadline(
      std::chrono::system_clock::now() +
      std::chrono::seconds(timeout_sec));
  return ctx;
}

std::string GrpcClient::ElementToJson(
    const aurum::Element& e) {
  std::ostringstream ss;
  ss << "{";
  ss << JStr("id", e.elementid()) << ", ";
  ss << JStr("text", e.text()) << ", ";
  ss << JStr("type", e.widgettype()) << ", ";
  ss << JStr("role", e.role()) << ", ";
  ss << JStr("package", e.package()) << ", ";
  ss << JStr("automationId", e.automationid())
     << ", ";
  ss << JStr("description", e.description())
     << ", ";
  ss << JStr("style", e.widgetstyle()) << ", ";
  ss << JBool("isEnabled", e.isenabled()) << ", ";
  ss << JBool("isFocused", e.isfocused()) << ", ";
  ss << JBool("isFocusable", e.isfocusable())
     << ", ";
  ss << JBool("isClickable", e.isclickable())
     << ", ";
  ss << JBool("isChecked", e.ischecked()) << ", ";
  ss << JBool("isCheckable", e.ischeckable())
     << ", ";
  ss << JBool("isScrollable", e.isscrollable())
     << ", ";
  ss << JBool("isVisible", e.isvisible()) << ", ";
  ss << JBool("isShowing", e.isshowing()) << ", ";
  ss << JBool("isActive", e.isactive()) << ", ";
  ss << "\"geometry\": {"
     << JInt("x", e.geometry().x()) << ", "
     << JInt("y", e.geometry().y()) << ", "
     << JInt("width", e.geometry().width()) << ", "
     << JInt("height", e.geometry().height())
     << "}";
  ss << "}";
  return ss.str();
}

void GrpcClient::PopulateFindRequest(
    aurum::ReqFindElement* req,
    const std::string& text,
    const std::string& text_partial,
    const std::string& id,
    const std::string& type,
    const std::string& role,
    const std::string& automation_id,
    const std::string& pkg,
    const std::string& description) {
  if (!text.empty())
    req->set_textfield(text);
  if (!text_partial.empty())
    req->set_textpartialmatch(text_partial);
  if (!id.empty())
    req->set_elementid(id);
  if (!type.empty())
    req->set_widgettype(type);
  if (!role.empty())
    req->set_role(role);
  if (!automation_id.empty())
    req->set_automationid(automation_id);
  if (!pkg.empty())
    req->set_packagename(pkg);
  if (!description.empty())
    req->set_description(description);
}

// --- Screen ---

std::string GrpcClient::GetScreenSize() {
  auto ctx = MakeCtx();
  aurum::ReqGetScreenSize req;
  aurum::RspGetScreenSize rsp;
  auto s = stub_->getScreenSize(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" + JInt("width", rsp.size().width()) +
         ", " +
         JInt("height", rsp.size().height()) + "}";
}

std::string GrpcClient::GetAngle() {
  auto ctx = MakeCtx();
  aurum::ReqGetAngle req;
  aurum::RspGetAngle rsp;
  auto s = stub_->getAngle(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JInt("windowAngle", rsp.windowangle()) +
         ", " +
         JInt("targetAngle", rsp.targetangle()) +
         "}";
}

std::string GrpcClient::GetDeviceTime(
    const std::string& type) {
  auto ctx = MakeCtx();
  aurum::ReqGetDeviceTime req;
  // Proto enum: WALLCLOCK=0, SYSTEM=1
  req.set_type(type == "monotonic"
                   ? aurum::ReqGetDeviceTime::SYSTEM
                   : aurum::ReqGetDeviceTime::WALLCLOCK);
  aurum::RspGetDeviceTime rsp;
  auto s = stub_->getDeviceTime(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JLong("timestamp", rsp.timestamputc()) +
         ", " + JStr("type", type) + "}";
}

std::string GrpcClient::TakeScreenshot(
    const std::string& path) {
  auto ctx = MakeCtx();
  aurum::ReqTakeScreenshot req;
  // Proto: bool getPixels = 1;
  // We request pixel data so we can write to file
  req.set_getpixels(true);
  auto reader = stub_->takeScreenshot(ctx.get(), req);

  // Collect screenshot data chunks
  aurum::RspTakeScreenshot rsp;
  std::string data;
  while (reader->Read(&rsp)) {
    // Proto: bytes image = 1;
    data += rsp.image();
  }
  auto s = reader->Finish();
  if (!s.ok()) return GrpcErr(s);

  // Write to file if we got data
  if (!data.empty() && !path.empty()) {
    FILE* f = fopen(path.c_str(), "wb");
    if (f) {
      fwrite(data.data(), 1, data.size(), f);
      fclose(f);
    }
  }
  return "{" + JBool("success", !data.empty()) +
         ", " + JStr("path", path) + "}";
}

// --- Input ---

std::string GrpcClient::Click(int x, int y,
                                int duration_ms) {
  auto ctx = MakeCtx();
  aurum::ReqClick req;
  auto* coord = req.mutable_coordination();
  coord->set_x(x);
  coord->set_y(y);
  req.set_type(aurum::ReqClick::COORD);
  aurum::RspClick rsp;
  auto s = (duration_ms > 0)
               ? stub_->longClick(ctx.get(), req, &rsp)
               : stub_->click(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         ", " + JInt("x", x) + ", " +
         JInt("y", y) + "}";
}

std::string GrpcClient::Flick(int sx, int sy,
                                int ex, int ey,
                                int steps,
                                int duration_ms) {
  auto ctx = MakeCtx();
  aurum::ReqFlick req;
  auto* sp = req.mutable_startpoint();
  sp->set_x(sx);
  sp->set_y(sy);
  auto* ep = req.mutable_endpoint();
  ep->set_x(ex);
  ep->set_y(ey);
  // Proto: int32 durationMs = 3;
  req.set_durationms(duration_ms);
  aurum::RspFlick rsp;
  auto s = stub_->flick(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::SendKey(
    const std::string& key,
    const std::string& action) {
  auto ctx = MakeCtx();
  aurum::ReqKey req;

  // Map key name to KeyType enum + XF86 keycode
  if (key == "back")
    req.set_type(aurum::ReqKey::BACK);
  else if (key == "home")
    req.set_type(aurum::ReqKey::HOME);
  else if (key == "menu")
    req.set_type(aurum::ReqKey::MENU);
  else if (key == "volup")
    req.set_type(aurum::ReqKey::VOLUP);
  else if (key == "voldown")
    req.set_type(aurum::ReqKey::VOLDOWN);
  else if (key == "power")
    req.set_type(aurum::ReqKey::POWER);
  else {
    // Custom XF86 keycode
    req.set_type(aurum::ReqKey::XF86);
    req.set_xf86keycode(key);
  }

  if (action == "press")
    req.set_actiontype(aurum::ReqKey::PRESS);
  else if (action == "release")
    req.set_actiontype(aurum::ReqKey::RELEASE);
  else
    req.set_actiontype(aurum::ReqKey::STROKE);

  aurum::RspKey rsp;
  auto s = stub_->sendKey(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         ", " + JStr("key", key) + ", " +
         JStr("action", action) + "}";
}

std::string GrpcClient::TouchDown(int x, int y) {
  auto ctx = MakeCtx();
  aurum::ReqTouchDown req;
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  aurum::RspTouchDown rsp;
  auto s = stub_->touchDown(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" + JInt("seqId", rsp.seqid()) + ", " +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::TouchMove(int x, int y,
                                    int seq) {
  auto ctx = MakeCtx();
  aurum::ReqTouchMove req;
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  req.set_seqid(seq);
  aurum::RspTouchMove rsp;
  auto s = stub_->touchMove(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::TouchUp(int x, int y,
                                  int seq) {
  auto ctx = MakeCtx();
  aurum::ReqTouchUp req;
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  req.set_seqid(seq);
  aurum::RspTouchUp rsp;
  auto s = stub_->touchUp(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::MouseDown(int x, int y,
                                    int button) {
  auto ctx = MakeCtx();
  aurum::ReqMouseDown req;
  req.set_button(button);
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  aurum::RspMouseDown rsp;
  auto s = stub_->mouseDown(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::MouseMove(int x, int y,
                                    int button) {
  auto ctx = MakeCtx();
  aurum::ReqMouseMove req;
  req.set_button(button);
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  aurum::RspMouseMove rsp;
  auto s = stub_->mouseMove(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::MouseUp(int x, int y,
                                  int button) {
  auto ctx = MakeCtx();
  aurum::ReqMouseUp req;
  req.set_button(button);
  auto* pt = req.mutable_coordination();
  pt->set_x(x);
  pt->set_y(y);
  aurum::RspMouseUp rsp;
  auto s = stub_->mouseUp(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

// --- Element ---

std::string GrpcClient::FindElement(
    const std::string& text,
    const std::string& text_partial,
    const std::string& id,
    const std::string& type,
    const std::string& role,
    const std::string& automation_id,
    const std::string& pkg,
    const std::string& description) {
  auto ctx = MakeCtx();
  aurum::ReqFindElement req;
  PopulateFindRequest(&req, text, text_partial,
                      id, type, role, automation_id,
                      pkg, description);
  aurum::RspFindElement rsp;
  auto s = stub_->findElement(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  // Proto: RspFindElement has singular element
  if (rsp.status() != aurum::OK ||
      !rsp.has_element())
    return "{\"error\": \"Element not found\"}";
  return ElementToJson(rsp.element());
}

std::string GrpcClient::FindElements(
    const std::string& text,
    const std::string& text_partial,
    const std::string& id,
    const std::string& type,
    const std::string& role,
    const std::string& automation_id,
    const std::string& pkg,
    const std::string& description) {
  auto ctx = MakeCtx();
  // ReqFindElements has same fields as
  // ReqFindElement (not a wrapper)
  aurum::ReqFindElements req;
  if (!text.empty())
    req.set_textfield(text);
  if (!text_partial.empty())
    req.set_textpartialmatch(text_partial);
  if (!id.empty())
    req.set_elementid(id);
  if (!type.empty())
    req.set_widgettype(type);
  if (!role.empty())
    req.set_role(role);
  if (!automation_id.empty())
    req.set_automationid(automation_id);
  if (!pkg.empty())
    req.set_packagename(pkg);
  if (!description.empty())
    req.set_description(description);

  aurum::RspFindElements rsp;
  auto s = stub_->findElements(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);

  std::ostringstream ss;
  ss << "{\"count\": " << rsp.elements_size()
     << ", \"elements\": [";
  for (int i = 0; i < rsp.elements_size(); ++i) {
    if (i > 0) ss << ", ";
    ss << ElementToJson(rsp.elements(i));
  }
  ss << "]}";
  return ss.str();
}

std::string GrpcClient::DumpTree() {
  auto ctx = MakeCtx();
  aurum::ReqDumpObjectTree req;
  aurum::RspDumpObjectTree rsp;
  auto s = stub_->dumpObjectTree(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);

  std::ostringstream ss;
  ss << "{\"roots\": [";
  for (int i = 0; i < rsp.roots_size(); ++i) {
    if (i > 0) ss << ", ";
    ss << ElementToJson(rsp.roots(i));
  }
  ss << "]}";
  return ss.str();
}

// --- Element actions ---

std::string GrpcClient::SetFocus(
    const std::string& element_id) {
  auto ctx = MakeCtx();
  aurum::ReqSetFocus req;
  req.set_elementid(element_id);
  aurum::RspSetFocus rsp;
  auto s = stub_->setFocus(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::DoAction(
    const std::string& element_id,
    const std::string& action) {
  auto ctx = MakeCtx();
  aurum::ReqDoAction req;
  req.set_elementid(element_id);
  // Proto: string action = 2;
  req.set_action(action);
  aurum::RspDoAction rsp;
  auto s = stub_->doAction(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

std::string GrpcClient::SetValue(
    const std::string& element_id,
    const std::string& text_val,
    double num_val) {
  auto ctx = MakeCtx();
  aurum::ReqSetValue req;
  req.set_elementid(element_id);
  if (!text_val.empty()) {
    req.set_stringvalue(text_val);
    req.set_type(aurum::ParamType::STRING);
  } else {
    req.set_doublevalue(num_val);
    req.set_type(aurum::ParamType::DOUBLE);
  }
  aurum::RspSetValue rsp;
  auto s = stub_->setValue(ctx.get(), req, &rsp);
  if (!s.ok()) return GrpcErr(s);
  return "{" +
         JBool("success",
                rsp.status() == aurum::OK) +
         "}";
}

}  // namespace aurum_cli
