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

#include "sensor_controller.hh"

#include <sensor.h>
#include <unistd.h>

#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

// clang-format off
const std::map<std::string, sensor_type_e> kTypes = {
  {"accelerometer",
      SENSOR_ACCELEROMETER},
  {"gravity",           SENSOR_GRAVITY},
  {"linear_acceleration",
      SENSOR_LINEAR_ACCELERATION},
  {"magnetic",          SENSOR_MAGNETIC},
  {"rotation_vector",   SENSOR_ROTATION_VECTOR},
  {"orientation",       SENSOR_ORIENTATION},
  {"gyroscope",         SENSOR_GYROSCOPE},
  {"light",             SENSOR_LIGHT},
  {"proximity",         SENSOR_PROXIMITY},
  {"pressure",          SENSOR_PRESSURE},
};
// clang-format on

constexpr const char* kAccNames[] = {
    "undefined", "unreliable", "low", "medium", "high"};

std::vector<std::string> GetValueKeys(
    const std::string& type, int value_count) {
  if (type == "accelerometer" || type == "gravity" ||
      type == "linear_acceleration" ||
      type == "magnetic" || type == "gyroscope")
    return {"x", "y", "z"};

  if (type == "orientation" ||
      type == "rotation_vector")
    return {"x", "y", "z", "w"};

  if (type == "light")
    return {"lux"};

  if (type == "proximity")
    return {"distance"};

  if (type == "pressure")
    return {"hpa"};

  std::vector<std::string> keys;
  for (int i = 0; i < value_count; ++i)
    keys.push_back("v" + std::to_string(i));

  return keys;
}

}  // namespace

std::string SensorController::Read(
    const std::string& type) const {
  auto it = kTypes.find(type);
  if (it == kTypes.end()) {
    return "{\"error\": \"Unknown sensor: " +
           type + "\"}";
  }

  sensor_h sensor = nullptr;
  if (sensor_get_default_sensor(
          it->second, &sensor) != 0)
    return "{\"error\": \"Sensor not available\"}";

  sensor_listener_h listener = nullptr;
  if (sensor_create_listener(sensor, &listener) != 0)
    return "{\"error\": \"Failed to create listener\"}";

  sensor_listener_start(listener);
  usleep(200000);

  sensor_event_s event;
  memset(&event, 0, sizeof(event));
  int ret = sensor_listener_read_data(
      listener, &event);

  sensor_listener_stop(listener);
  sensor_destroy_listener(listener);

  if (ret != 0)
    return "{\"error\": \"Failed to read sensor\"}";

  auto keys = GetValueKeys(type, event.value_count);
  std::string acc_str =
      (event.accuracy >= 0 && event.accuracy <= 4)
          ? kAccNames[event.accuracy]
          : "unknown";

  std::string r =
      "{\"sensor_type\": \"" + type +
      "\", \"values\": {";
  int n = std::min(
      static_cast<int>(keys.size()),
      event.value_count);

  for (int i = 0; i < n; ++i) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%.4f",
             event.values[i]);
    if (i > 0)
      r += ", ";

    r += "\"" + keys[i] + "\": " + buf;
  }

  r += "}, \"accuracy\": \"" + acc_str +
       "\", \"timestamp\": " +
       std::to_string(event.timestamp) + "}";
  return r;
}

}  // namespace cli
}  // namespace tizenclaw
