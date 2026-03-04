#include "tizenclaw.h"

#include <iostream>
#include <string>
#include <csignal>
#include <exception>

TizenClawDaemon* g_daemon = nullptr;

void signal_handler(int sig) {
    dlog_print(DLOG_INFO, LOG_TAG, "Caught signal %d", sig);
    if (g_daemon) {
        g_daemon->Quit();
    }
}

TizenClawDaemon::TizenClawDaemon(int argc, char** argv)
    : argc_(argc), argv_(argv) {
    tizen_core_init();
    tizen_core_task_create("main", false, &task_);
}

TizenClawDaemon::~TizenClawDaemon() {
    if (task_) {
        tizen_core_task_destroy(task_);
        task_ = nullptr;
    }
    tizen_core_shutdown();
}

int TizenClawDaemon::Run() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon Run");
    OnCreate();
    
    // Set up signal handling
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    int ret = tizen_core_task_run(task_);
    OnDestroy();
    return ret;
}

void TizenClawDaemon::Quit() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon Quit");
    if (task_) {
        tizen_core_task_quit(task_);
    }
}

void TizenClawDaemon::OnCreate() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon OnCreate");
    agent_ = new AgentCore();
    if (!agent_->Initialize()) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to initialize AgentCore");
    }

    // TODO: Initialize LXC Container Engine
    // TODO: Start MCP Server connection
}

void TizenClawDaemon::OnDestroy() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon OnDestroy");
    if (agent_) {
        agent_->Shutdown();
        delete agent_;
        agent_ = nullptr;
    }
    
    // TODO: Cleanup LXC processes and MCP sockets here
}

int main(int argc, char *argv[]) {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Service starting...");
    try {
        TizenClawDaemon daemon(argc, argv);
        g_daemon = &daemon;
        return daemon.Run();
    } catch (const std::exception& e) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Exception: %s", e.what());
        return -1;
    } catch (...) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Unknown exception");
        return -1;
    }
}
