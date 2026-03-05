#include "telegram_bridge.hh"

#include <dlog.h>
#include <fstream>
#include <sstream>
#include <cstring>
#include <csignal>
#include <sys/wait.h>
#include <unistd.h>
#include <json.hpp>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Telegram"

#ifndef APP_DATA_DIR
#define APP_DATA_DIR "/opt/usr/share/tizenclaw"
#endif

TelegramBridge::TelegramBridge()
    : python_bin_("python3"),
      listener_script_(std::string(APP_DATA_DIR) +
                       "/skills/telegram_listener/telegram_listener.py"),
      config_path_(std::string(APP_DATA_DIR) +
                   "/telegram_config.json") {
}

TelegramBridge::~TelegramBridge() {
    Stop();
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

bool TelegramBridge::Start() {
    if (running_) {
        dlog_print(DLOG_WARN, LOG_TAG,
                   "TelegramBridge already running");
        return true;
    }

    if (!LoadConfig()) {
        dlog_print(DLOG_WARN, LOG_TAG,
                   "Telegram config not found or invalid. "
                   "Telegram bridge disabled.");
        return false;
    }

    if (!SpawnListener()) {
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "Failed to spawn telegram_listener");
        return false;
    }

    running_ = true;
    restart_count_ = 0;
    watchdog_thread_ =
        std::thread(&TelegramBridge::WatchdogLoop, this);
    dlog_print(DLOG_INFO, LOG_TAG,
               "TelegramBridge started (pid=%d)",
               child_pid_);
    return true;
}

void TelegramBridge::Stop() {
    running_ = false;

    if (child_pid_ > 0) {
        dlog_print(DLOG_INFO, LOG_TAG,
                   "Stopping telegram_listener (pid=%d)",
                   child_pid_);

        // Graceful shutdown: SIGTERM first
        kill(child_pid_, SIGTERM);

        // Wait up to 2 seconds for the child to exit
        for (int i = 0; i < 20; ++i) {
            int status = 0;
            pid_t ret =
                waitpid(child_pid_, &status, WNOHANG);
            if (ret == child_pid_ || ret < 0) {
                child_pid_ = -1;
                break;
            }
            usleep(100 * 1000);  // 100ms
        }

        // Force kill if still alive
        if (child_pid_ > 0) {
            dlog_print(DLOG_WARN, LOG_TAG,
                       "Force-killing telegram_listener "
                       "(pid=%d)", child_pid_);
            kill(child_pid_, SIGKILL);
            waitpid(child_pid_, nullptr, 0);
            child_pid_ = -1;
        }
    }

    if (watchdog_thread_.joinable()) {
        watchdog_thread_.join();
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "TelegramBridge stopped");
}

bool TelegramBridge::IsRunning() const {
    if (child_pid_ <= 0) return false;

    int status = 0;
    pid_t ret =
        waitpid(child_pid_, &status, WNOHANG);
    return (ret == 0);  // 0 means child still running
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

bool TelegramBridge::LoadConfig() {
    std::ifstream ifs(config_path_);
    if (!ifs.is_open()) {
        dlog_print(DLOG_WARN, LOG_TAG,
                   "Cannot open %s",
                   config_path_.c_str());
        return false;
    }

    try {
        nlohmann::json cfg =
            nlohmann::json::parse(ifs);
        bot_token_ =
            cfg.value("bot_token", "");
        if (bot_token_.empty() ||
            bot_token_ == "YOUR_TELEGRAM_BOT_TOKEN_HERE") {
            dlog_print(DLOG_WARN, LOG_TAG,
                       "bot_token is empty or placeholder");
            return false;
        }
    } catch (const nlohmann::json::exception& e) {
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "Failed to parse %s: %s",
                   config_path_.c_str(), e.what());
        return false;
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "Telegram config loaded (token length=%zu)",
               bot_token_.size());
    return true;
}

bool TelegramBridge::SpawnListener() {
    pid_t pid = fork();

    if (pid < 0) {
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "fork() failed: %s",
                   strerror(errno));
        return false;
    }

    if (pid == 0) {
        // ---- Child process ----

        // Set TELEGRAM_BOT_TOKEN environment variable
        setenv("TELEGRAM_BOT_TOKEN",
               bot_token_.c_str(), 1);

        // Close inherited sockets / file descriptors
        // (keep stdin/out/err for logging)
        for (int fd = 3; fd < 1024; ++fd) {
            close(fd);
        }

        // exec python3 telegram_listener.py
        execl(python_bin_.c_str(),
              python_bin_.c_str(),
              listener_script_.c_str(),
              nullptr);

        // If execl returns, something went wrong
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "execl() failed: %s",
                   strerror(errno));
        _exit(127);
    }

    // ---- Parent process ----
    child_pid_ = pid;
    dlog_print(DLOG_INFO, LOG_TAG,
               "Spawned telegram_listener (pid=%d)",
               child_pid_);
    return true;
}

void TelegramBridge::WatchdogLoop() {
    dlog_print(DLOG_INFO, LOG_TAG,
               "Watchdog thread started");

    while (running_) {
        // Sleep 1 second between checks
        for (int i = 0; i < 10 && running_; ++i) {
            usleep(100 * 1000);
        }

        if (!running_ || child_pid_ <= 0) break;

        int status = 0;
        pid_t ret =
            waitpid(child_pid_, &status, WNOHANG);

        if (ret == 0) {
            // Child still running, all good
            continue;
        }

        if (ret == child_pid_) {
            // Child exited
            if (WIFEXITED(status)) {
                dlog_print(DLOG_WARN, LOG_TAG,
                    "telegram_listener exited "
                    "with code %d",
                    WEXITSTATUS(status));
            } else if (WIFSIGNALED(status)) {
                dlog_print(DLOG_WARN, LOG_TAG,
                    "telegram_listener killed "
                    "by signal %d",
                    WTERMSIG(status));
            }
            child_pid_ = -1;
        } else if (ret < 0) {
            dlog_print(DLOG_WARN, LOG_TAG,
                       "waitpid() error: %s",
                       strerror(errno));
            child_pid_ = -1;
        }

        // Attempt restart if within limits
        if (!running_) break;

        if (restart_count_ >= kMaxRestarts) {
            dlog_print(DLOG_ERROR, LOG_TAG,
                "telegram_listener exceeded max "
                "restarts (%d). Giving up.",
                kMaxRestarts);
            running_ = false;
            break;
        }

        ++restart_count_;
        dlog_print(DLOG_INFO, LOG_TAG,
            "Restarting telegram_listener "
            "(attempt %d/%d) in %d seconds...",
            restart_count_, kMaxRestarts,
            kRestartDelaySec);

        // Wait kRestartDelaySec before restarting
        for (int i = 0;
             i < kRestartDelaySec * 10 && running_;
             ++i) {
            usleep(100 * 1000);
        }

        if (!running_) break;

        if (!SpawnListener()) {
            dlog_print(DLOG_ERROR, LOG_TAG,
                "Failed to respawn "
                "telegram_listener");
            running_ = false;
            break;
        }

        dlog_print(DLOG_INFO, LOG_TAG,
            "telegram_listener restarted "
            "(pid=%d)", child_pid_);
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "Watchdog thread exiting");
}
