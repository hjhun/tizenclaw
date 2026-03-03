#include "tizenclaw.h"

#include <iostream>
#include <string>

// Entry point when the service is created
bool service_app_create(void *data) {
    appdata *ad = static_cast<appdata *>(data);
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Service Created.");

    ad->is_running = true;

    // TODO: Initialize Agent Core (Planner)
    // TODO: Initialize LXC Container Engine
    // TODO: Start MCP Server connection

    return true;
}

// Entry point when the service is terminated
void service_app_terminate(void *data) {
    appdata *ad = static_cast<appdata *>(data);
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Service Terminated.");

    ad->is_running = false;

    // TODO: Cleanup LXC processes and MCP sockets here
}

// Entry point when another app sends an AppControl request (e.g. Prompt intent)
void service_app_control(app_control_h app_control, void *data) {
    appdata *ad = static_cast<appdata *>(data);
    if (!ad) return;

    char *caller_id = nullptr;
    if (app_control_get_caller(app_control, &caller_id) == APP_CONTROL_ERROR_NONE) {
        dlog_print(DLOG_INFO, LOG_TAG, "AppControl received from: %s", caller_id);
    }
    
    char *operation = nullptr;
    if (app_control_get_operation(app_control, &operation) == APP_CONTROL_ERROR_NONE) {
        dlog_print(DLOG_INFO, LOG_TAG, "Operation: %s", operation);

        // Here we will eventually intercept commands, parse intents, and pass them to Agent Core
    }

    if (caller_id) free(caller_id);
    if (operation) free(operation);
}

void service_app_lang_changed(app_event_info_h event_info, void *user_data) {
    // App event handlers
}

void service_app_region_changed(app_event_info_h event_info, void *user_data) {
    // App event handlers
}

int main(int argc, char *argv[]) {
    appdata ad = {false};
    service_app_lifecycle_callback_s event_callback = {0,};

    event_callback.create = service_app_create;
    event_callback.terminate = service_app_terminate;
    event_callback.app_control = service_app_control;

    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Service starting...");

    int result = service_app_main(argc, argv, &event_callback, &ad);
    
    if (result != APP_ERROR_NONE)
        dlog_print(DLOG_ERROR, LOG_TAG, "Service app starting failed: %d", result);

    return result;
}
