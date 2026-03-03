#ifndef __TIZENCLAW_H__
#define __TIZENCLAW_H__

#include <dlog.h>
#include <service_app.h>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw"

// Application state structure
struct appdata {
    // Add runtime engine variables, LXC references, MCP configs here
    bool is_running;
};

// Lifecycle callbacks
bool service_app_create(void *data);
void service_app_terminate(void *data);
void service_app_control(app_control_h app_control, void *data);
void service_app_lang_changed(app_event_info_h event_info, void *user_data);
void service_app_region_changed(app_event_info_h event_info, void *user_data);

#endif // __TIZENCLAW_H__
