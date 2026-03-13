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

#ifndef AURUM_GRPC_CLIENT_HH_
#define AURUM_GRPC_CLIENT_HH_

#include <memory>
#include <string>
#include <vector>

#include <grpcpp/grpcpp.h>
#include "aurum.grpc.pb.h"

namespace aurum_cli {

class GrpcClient {
 public:
  explicit GrpcClient(const std::string& addr);

  // Screen
  std::string GetScreenSize();
  std::string GetAngle();
  std::string GetDeviceTime(const std::string& type);
  std::string TakeScreenshot(const std::string& path);

  // Input
  std::string Click(int x, int y, int duration_ms);
  std::string Flick(int sx, int sy, int ex, int ey,
                     int steps, int duration_ms);
  std::string SendKey(const std::string& key,
                       const std::string& action);
  std::string TouchDown(int x, int y);
  std::string TouchMove(int x, int y, int seq);
  std::string TouchUp(int x, int y, int seq);
  std::string MouseDown(int x, int y, int button);
  std::string MouseMove(int x, int y, int button);
  std::string MouseUp(int x, int y, int button);

  // Element
  std::string FindElement(
      const std::string& text,
      const std::string& text_partial,
      const std::string& id,
      const std::string& type,
      const std::string& role,
      const std::string& automation_id,
      const std::string& pkg,
      const std::string& description);
  std::string FindElements(
      const std::string& text,
      const std::string& text_partial,
      const std::string& id,
      const std::string& type,
      const std::string& role,
      const std::string& automation_id,
      const std::string& pkg,
      const std::string& description);
  std::string DumpTree();

  // Element actions
  std::string SetFocus(const std::string& element_id);
  std::string DoAction(const std::string& element_id,
                        const std::string& action);
  std::string SetValue(const std::string& element_id,
                        const std::string& text_val,
                        double num_val);

 private:
  std::unique_ptr<aurum::Bootstrap::Stub> stub_;

  // Create a ClientContext with a deadline
  std::unique_ptr<grpc::ClientContext>
  MakeCtx(int timeout_sec = 10);

  // Helper: serialise Element proto to JSON
  std::string ElementToJson(
      const aurum::Element& elem);

  // Helper: populate find request fields
  void PopulateFindRequest(
      aurum::ReqFindElement* req,
      const std::string& text,
      const std::string& text_partial,
      const std::string& id,
      const std::string& type,
      const std::string& role,
      const std::string& automation_id,
      const std::string& pkg,
      const std::string& description);
};

}  // namespace aurum_cli

#endif
