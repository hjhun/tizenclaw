#!/usr/bin/env python3
import ctypes
import json
import os
import sys
import time

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# tone_type_e values
TONE_TYPES = {
    "DTMF_0": 0, "DTMF_1": 1, "DTMF_2": 2, "DTMF_3": 3,
    "DTMF_4": 4, "DTMF_5": 5, "DTMF_6": 6, "DTMF_7": 7,
    "DTMF_8": 8, "DTMF_9": 9, "DTMF_S": 10, "DTMF_P": 11,
    "SUP_DIAL": 16, "SUP_BUSY": 17, "SUP_CONGESTION": 18,
    "SUP_RADIO_ACK": 19, "SUP_RADIO_NA": 20, "SUP_ERROR": 21,
    "SUP_CALL_WAITING": 22, "SUP_RINGTONE": 23,
    "PROP_BEEP": 24, "PROP_ACK": 25, "PROP_NACK": 26,
    "PROP_PROMPT": 27, "PROP_BEEP2": 28,
}


def play_tone(tone_name, duration_ms=500):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-media-tone-player.so.0",
             "libcapi-media-tone-player.so.1"]
        )

        # int tone_player_start_new(tone_type_e tone, sound_stream_info_h stream_info,
        #   int duration_ms, int *id)
        lib.tone_player_start_new.argtypes = [
            ctypes.c_int, ctypes.c_void_p, ctypes.c_int,
            ctypes.POINTER(ctypes.c_int)
        ]
        lib.tone_player_start_new.restype = ctypes.c_int

        # int tone_player_stop(int id)
        lib.tone_player_stop.argtypes = [ctypes.c_int]
        lib.tone_player_stop.restype = ctypes.c_int

        # We need a sound stream info handle
        sm_lib = tizen_capi_utils.load_library(
            ["libcapi-media-sound-manager.so.0",
             "libcapi-media-sound-manager.so.1"]
        )

        # int sound_manager_create_stream_information(
        #   sound_stream_type_e type, sound_stream_focus_state_changed_cb cb,
        #   void *user_data, sound_stream_info_h *stream_info)
        sm_lib.sound_manager_create_stream_information.argtypes = [
            ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_void_p)
        ]
        sm_lib.sound_manager_create_stream_information.restype = ctypes.c_int

        sm_lib.sound_manager_destroy_stream_information.argtypes = [ctypes.c_void_p]
        sm_lib.sound_manager_destroy_stream_information.restype = ctypes.c_int

        tone_val = TONE_TYPES.get(tone_name.upper())
        if tone_val is None:
            return {"error": f"Unknown tone: {tone_name}"}

        # Create stream info (type=3 is SOUND_STREAM_TYPE_MEDIA)
        stream_info = ctypes.c_void_p()
        ret = sm_lib.sound_manager_create_stream_information(
            3, None, None, ctypes.byref(stream_info)
        )
        if ret != 0:
            return {"error": f"Failed to create stream info (code: {ret})"}

        tone_id = ctypes.c_int()
        ret = lib.tone_player_start_new(
            tone_val, stream_info, duration_ms, ctypes.byref(tone_id)
        )

        if ret != 0:
            sm_lib.sound_manager_destroy_stream_information(stream_info)
            return {"error": f"Failed to play tone (code: {ret})"}

        # Wait for tone to finish
        time.sleep(duration_ms / 1000.0 + 0.1)

        lib.tone_player_stop(tone_id)
        sm_lib.sound_manager_destroy_stream_information(stream_info)

        return {
            "status": "success",
            "tone": tone_name.upper(),
            "duration_ms": duration_ms,
            "message": f"Played tone {tone_name.upper()} for {duration_ms}ms",
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    t = args.get("tone", "PROP_BEEP")
    dur = args.get("duration_ms", 500)
    print(json.dumps(play_tone(t, int(dur))))
