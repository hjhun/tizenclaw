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
#ifndef FILE_LOG_BACKEND_HH
#define FILE_LOG_BACKEND_HH

#include <memory>
#include <mutex>
#include <string>

#include "logging.hh"

namespace tizenclaw {

namespace utils {

class FileLogBackend : public ILogBackend {
 public:
  FileLogBackend(std::string file_path, int rotation_size, int max_rotation);

  void WriteLog(LogLevel level, const std::string& tag,
                const std::string& logstr) override;

 private:
  bool Rotate();
  int GetFileSize(const std::string& file_path);
  std::string GetTimeStamp();
  std::string GetPid();
  std::string GetLogDir();
  std::string GetFileName();

 private:
  std::string file_path_;
  int rotation_size_;
  int max_rotation_;
  std::mutex mutex_;
};

}  // namespace utils

}  // namespace tizenclaw

#endif  // FILE_LOG_BACKEND_HH