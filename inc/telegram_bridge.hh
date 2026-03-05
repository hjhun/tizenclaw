#ifndef __TELEGRAM_BRIDGE_H__
#define __TELEGRAM_BRIDGE_H__

#include <dlog.h>
#include <string>
#include <thread>
#include <atomic>
#include <sys/types.h>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Telegram"

/**
 * @brief Manages the telegram_listener.py process as a child of tizenclaw.
 *
 * On Start(), reads bot token from telegram_config.json, forks, and execs
 * python3 telegram_listener.py.  A watchdog thread monitors the child and
 * restarts it up to kMaxRestarts times with kRestartDelaySec interval.
 */
class TelegramBridge {
public:
    TelegramBridge();
    ~TelegramBridge();

    /**
     * @brief Load config and spawn the listener process.
     * @return true if telegram_config.json was found and the child was
     *         forked successfully; false otherwise (non-fatal).
     */
    bool Start();

    /**
     * @brief Stop the listener process and watchdog thread.
     *        Sends SIGTERM first, waits 2 s, then SIGKILL if needed.
     */
    void Stop();

    /**
     * @brief Non-blocking check: is the child still alive?
     */
    bool IsRunning() const;

private:
    void WatchdogLoop();
    bool LoadConfig();
    bool SpawnListener();

    pid_t child_pid_ = -1;
    std::string bot_token_;
    std::string python_bin_;
    std::string listener_script_;
    std::string config_path_;

    std::thread watchdog_thread_;
    std::atomic<bool> running_{false};

    static constexpr int kMaxRestarts = 3;
    static constexpr int kRestartDelaySec = 5;
    int restart_count_ = 0;
};

#endif // __TELEGRAM_BRIDGE_H__
