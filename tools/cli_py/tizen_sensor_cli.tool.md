---
name: tizen-sensor-cli
description: "Read sensor data from device sensors: accelerometer, gravity, gyroscope, light, proximity, pressure, magnetic, orientation"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_sensor_cli.py"
---
# tizen-sensor-cli

**Description**: Read sensor data from device sensors.

## Usage
```
tizen-sensor-cli --type accelerometer|gravity|gyroscope|light|proximity|pressure|magnetic|orientation
```

## Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `--type` | Yes | Sensor type to read |

## Supported Sensor Types

| Type | Description |
|------|-------------|
| `accelerometer` | X/Y/Z acceleration (m/s²) |
| `gravity` | Gravity vector |
| `gyroscope` | Angular velocity |
| `light` | Ambient light level (lux) |
| `proximity` | Proximity detection |
| `pressure` | Atmospheric pressure (hPa) |
| `magnetic` | Magnetic field |
| `orientation` | Device orientation (azimuth, pitch, roll) |

## Output
All output is JSON. Example:
```json
{"status": "success", "type": "accelerometer", "values": [0.1, 0.2, 9.8], "timestamp": 1234567890}
```
