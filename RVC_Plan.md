# RVC_Plan.md — How to Control Your Samsung Jet Bot with TizenClaw

**Who this guide is for:** Someone who wants to use AI to control their Samsung Jet Bot
robot vacuum — no technical background needed. Every step is explained in plain language.

---

## What is happening here? (Plain English)

Think of **TizenClaw** as a smart assistant that lives on your computer and listens to
your instructions in plain English — like "start the vacuum" or "dock the robot". It
understands what you mean and talks to Samsung's cloud service on your behalf to make
it happen.

To understand your instructions, TizenClaw needs an **AI brain**. This guide uses
**Ollama**, which is a completely free, open-source AI that runs entirely on your own
computer — your data never leaves your machine.

Here is the overall picture:

```
You type: "Start the vacuum"
       ↓
TizenClaw (the assistant on your computer)
       ↓  understands using Ollama AI  ← can run here OR on a second computer
       ↓  sends a command to Samsung's internet service
       ↓
SmartThings (Samsung's cloud service)
       ↓
Your Jet Bot starts cleaning
```

The Ollama AI can run on the **same computer** as TizenClaw (simpler), or on a
**different computer on your home Wi-Fi** (better performance, more flexibility).
Both options are covered in this guide.

---

## Before you start — What you will need

- [ ] Your computer is running Linux
- [ ] Your Samsung Jet Bot is already visible in the SmartThings app on your phone
- [ ] A stable internet connection
- [ ] About 30–60 minutes of time
- [ ] About 25 GB of free disk space (for the AI model)

---

## Can the AI brain run on a different computer?

**Yes — and this is a great idea.** If you have two computers on the same Wi-Fi, you
can run the Ollama AI on the more powerful one and TizenClaw on the other. See
**PART 1B** for how to do this. Otherwise, follow **PART 1A** to run everything on
one machine.

---

## PART 1A — Install the AI Brain (Ollama) on the same computer as TizenClaw

Ollama is a free program that runs an AI model on your own computer.
Think of it like downloading a smart assistant that works offline, for free, forever.

### Step 1.1 — Open a Terminal

A "terminal" is a text window where you can type commands to your computer.

- Press **Ctrl + Alt + T** on your keyboard, OR
- Search for "Terminal" in your application menu and open it

You will see a window with a blinking cursor. This is normal — it is waiting for you
to type commands.

### Step 1.2 — Install Ollama

In the terminal, type (or copy-paste) this single command and press **Enter**:

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

> **What this does:** It downloads and installs Ollama from the official website.
> You may be asked for your computer password — type it and press Enter
> (the password will not appear on screen as you type, that is normal).

Wait for it to finish. You will see "Installation successful" when done.

### Step 1.3 — Download the AI Model

You have 32 GB of RAM, which means you can run a very capable AI model called
**Qwen 2.5 (32 billion parameters)**. Think of it like choosing between a small
pocket calculator and a powerful laptop — you have the hardware for the powerful option.

Type this command and press **Enter**:

```bash
ollama pull qwen2.5:32b
```

> **What this does:** Downloads the AI model to your computer. This is about 20 GB
> and may take 10–30 minutes depending on your internet speed. The download shows a
> progress bar. Wait until it says "success".

### Step 1.4 — Verify Ollama is Working

Type this and press **Enter**:

```bash
ollama list
```

You should see `qwen2.5:32b` in the list. If you do, Ollama is ready.

---

---

## PART 1B — Run Ollama on a Different Computer (Optional)

> **Skip this part** if you are running everything on one machine. Jump straight to PART 2.

This setup is useful if:
- You have a more powerful second computer (especially one with a dedicated graphics card / GPU)
- You want the AI to be faster or handle more complex instructions
- You want to keep TizenClaw and the AI brain on separate machines

Here is what the setup looks like:

```
Your TizenClaw computer          Your Ollama computer (same Wi-Fi)
──────────────────────           ────────────────────────────────
Runs: tizenclaw-cli              Runs: the Ollama AI
      ↓                                ↑
      └── asks the AI ────────────────►┘ (over your home Wi-Fi)
```

### 1B-1 — On the Ollama computer: install Ollama and download the model

Follow Steps 1.1 through 1.3 from PART 1A (above) on that second computer.
When done, the AI model `qwen2.5:32b` will be downloaded on that machine.

### 1B-2 — On the Ollama computer: allow other computers to connect

By default, Ollama only talks to the computer it is running on — it ignores requests
from other computers on the network. We need to change this.

> **Why this matters:** It is like a shop that only lets people in through the back door.
> We need to open the front door so your TizenClaw computer can reach it.

Open a terminal on the **Ollama computer** and run:

```bash
sudo systemctl edit ollama
```

A text editor opens. It will be mostly blank. Type these three lines **exactly**:

```
[Service]
Environment="OLLAMA_HOST=0.0.0.0"
```

Save and exit: press **Ctrl+O**, then **Enter**, then **Ctrl+X**.

Now restart Ollama to apply the change:

```bash
sudo systemctl restart ollama
```

### 1B-3 — On the Ollama computer: open the network door (port)

A "port" is like a specific door number on your computer. Ollama uses door number
**11434**. We need to tell your firewall to allow traffic through this door:

```bash
sudo ufw allow 11434/tcp
```

If you see "command not found", skip this step — your firewall is not active and
the door is already open by default.

### 1B-4 — Find the Ollama computer's address on your network

Every device on your home Wi-Fi has a local address (like a house number on a street).
We need to find the Ollama computer's address.

On the **Ollama computer**, run:

```bash
ip route get 1.1.1.1 | awk '{print $7; exit}'
```

You will see something like `192.168.1.42`. **Write this number down** — you will
need it in the next step.

### 1B-5 — On the TizenClaw computer: point TizenClaw to the Ollama computer

After you have installed TizenClaw (PART 2) and configured the AI settings (PART 3),
open the AI settings file:

```bash
nano ~/.tizenclaw/config/llm_config.json
```

Find the `"ollama"` section and replace `localhost` with the address you wrote down:

**Before:**
```json
"ollama": {
  "model": "qwen2.5:32b",
  "endpoint": "http://localhost:11434"
}
```

**After** (use your actual number, not 192.168.1.42):
```json
"ollama": {
  "model": "qwen2.5:32b",
  "endpoint": "http://192.168.1.42:11434"
}
```

Save: **Ctrl+O**, **Enter**, **Ctrl+X**.

### 1B-6 — Check that it is working

From the **TizenClaw computer**, test the connection:

```bash
curl http://192.168.1.42:11434/api/tags
```

(Replace `192.168.1.42` with your actual Ollama computer address.)

You should see a list of AI models including `qwen2.5:32b`. If you see
"connection refused", go back and repeat Steps 1B-2 and 1B-3.

### Important notes for the two-computer setup

- Both computers must be connected to the **same Wi-Fi router**. It will not work
  if one is on mobile data.
- The **Ollama computer must be turned on** whenever you want to use the vacuum
  assistant. If it goes to sleep, TizenClaw cannot reach the AI.
- The Ollama computer does **not** need to have the vacuum tool or TizenClaw
  installed — it only runs the AI model.
- Ollama has **no password protection** — anyone on your home Wi-Fi could use it.
  This is fine at home but do not set this up in a shared office or café.

---

## PART 2 — Install TizenClaw

TizenClaw is the assistant program that bridges your instructions with the vacuum.
It is already downloaded on your computer in the folder you are working with.

### Step 2.1 — Install Required Build Tools

TizenClaw is written in a programming language called Rust. You need to install
Rust before you can build it. Type these commands one at a time, pressing **Enter**
after each:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://rustup.rs | sh
```

When it asks you to choose, press **Enter** to accept the default (option 1).

Then reload your terminal settings:

```bash
source "$HOME/.cargo/env"
```

Also install a few other required tools:

```bash
sudo apt-get update && sudo apt-get install -y \
  libcurl4-openssl-dev \
  pkg-config \
  libssl-dev \
  build-essential \
  jq
```

> **What `jq` is:** A helper tool that makes it easy to read data from Samsung's
> servers in the next steps.

### Step 2.2 — Go Into the TizenClaw Folder

```bash
cd /home/gagan/strawhats/tizen_claw/tizenclaw
```

> **What `cd` means:** "Change Directory" — like double-clicking a folder to open it.

### Step 2.3 — Build and Install TizenClaw

```bash
./deploy_host.sh
```

> **What this does:** Compiles TizenClaw (turns its source code into a working program)
> and installs it into `~/.tizenclaw/` on your computer. This takes 5–15 minutes the
> first time. You will see many lines of output — that is normal.

When it finishes, you will see a message saying TizenClaw has started.
For now, stop it immediately — we need to configure it first:

```bash
./deploy_host.sh --stop
```

---

## PART 3 — Connect TizenClaw to the Ollama AI (on TizenClaw's computer)

TizenClaw needs to know which AI brain to use. We will tell it to use Ollama.

> **Two-computer setup?** Follow all steps in this part on the **TizenClaw computer**
> (not the Ollama one). You will change the endpoint address in Step 3.3 below.

### Step 3.1 — Create the AI Configuration File

The settings file needs to be created from the sample. Type:

```bash
cp /home/gagan/strawhats/tizen_claw/tizenclaw/data/sample/llm_config.json.sample \
   ~/.tizenclaw/config/llm_config.json
```

### Step 3.2 — Open the File in a Text Editor

```bash
nano ~/.tizenclaw/config/llm_config.json
```

> **What `nano` is:** A simple text editor that opens inside the terminal.

You will see the contents of the file. It looks like this at the top:

```json
{
  "active_backend": "gemini",
```

### Step 3.3 — Change "gemini" to "ollama"

Using the arrow keys on your keyboard, move to the word `"gemini"` on the
`"active_backend"` line and change it to `"ollama"`:

```json
{
  "active_backend": "ollama",
```

Then scroll down (using the arrow keys) until you find the `"ollama"` section:

```json
"ollama": {
  "model": "llama3",
  "endpoint": "http://localhost:11434"
},
```

Change `"llama3"` to `"qwen2.5:32b"`:

```json
"ollama": {
  "model": "qwen2.5:32b",
  "endpoint": "http://localhost:11434"
},
```

> **Two-computer setup?** Also change `localhost` to the IP address of your Ollama
> computer (the one you found in Step 1B-4). Example:
> `"endpoint": "http://192.168.1.42:11434"`

### Step 3.4 — Save the File

Press **Ctrl + O** (the letter O, not zero), then press **Enter** to confirm saving.
Then press **Ctrl + X** to exit the editor.

---

## PART 4 — Get Permission to Talk to Your Vacuum

Samsung's SmartThings service uses a "token" (think of it like a temporary password)
to verify that TizenClaw is allowed to send commands to your vacuum.
You generate this token on Samsung's website.

> **Important:** This type of token expires after **24 hours**. This means you will
> need to repeat Steps 4.1–4.2 every day to get a fresh token. See Part 5 for how
> to update it.

### Step 4.1 — Create a Samsung Developer Account

1. Open your web browser and go to: **https://developer.smartthings.com**
2. Click **"Sign in"** in the top right
3. Sign in with the **same Samsung account** you use on your phone for SmartThings
4. If asked to create a developer profile, fill in your name and accept the terms

### Step 4.2 — Generate an Access Token (Your Password)

An "access token" is a long string of letters and numbers that acts like a temporary
password. It proves to Samsung's servers that you are allowed to control your vacuum.

1. Go to: **https://account.smartthings.com/tokens**
   (This is Samsung's token generation page)
2. Click the blue **"Generate new token"** button
3. In the **"Token name"** field, type anything you like — for example: `tizenclaw-vacuum`
4. Under **"Authorized Scopes"**, tick both of these boxes:
   - `Devices (read)` — this lets TizenClaw see your vacuum's status
   - `Devices (execute)` — this lets TizenClaw send commands to your vacuum
5. Click **"Generate token"**
6. A long string of letters and numbers will appear — **copy it immediately** and
   save it somewhere safe (like a text file on your Desktop). This is your access
   token. **Samsung will only show it once.**

Your token will look something like this (but much longer and different):
```
xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
```

### Step 4.3 — Find Your Vacuum's ID

Your Jet Bot has a unique identifier in Samsung's system — like a serial number
but for the internet. We need to find it.

Run this command in the terminal, replacing `YOUR_TOKEN_HERE` with the token
you copied in Step 4.2:

```bash
curl -s \
  -H "Authorization: Bearer YOUR_TOKEN_HERE" \
  "https://api.smartthings.com/v1/devices" | \
  jq '.items[] | {name: .label, id: .deviceId}'
```

> **What this does:** Asks Samsung's servers to list all your SmartThings devices
> and shows their names and IDs.

The output will look something like:

```json
{
  "name": "Jet Bot",
  "id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
{
  "name": "Living Room Light",
  "id": "yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy"
}
```

Find the entry for your Jet Bot and **copy its `id` value**. Save it alongside
your token.

---

## PART 5 — Fill In the Credentials File

Now we put your token and device ID into TizenClaw's configuration file so it
knows how to reach your vacuum.

### Step 5.1 — Open the Credentials File

```bash
nano ~/.tizenclaw/config/robotic_vacuum_config.json
```

You will see:

```json
{
  "client_id": "<SMARTTHINGS_CLIENT_ID>",
  "client_secret": "<SMARTTHINGS_CLIENT_SECRET>",
  "access_token": "<INITIAL_ACCESS_TOKEN>",
  "refresh_token": "<REFRESH_TOKEN>",
  "device_id": "<JET_BOT_DEVICE_ID>"
}
```

### Step 5.2 — Fill In Your Values

Replace the placeholder values with your actual information:

| Placeholder | What to put here |
|---|---|
| `<SMARTTHINGS_CLIENT_ID>` | Leave as `none` — not needed for token-based setup |
| `<SMARTTHINGS_CLIENT_SECRET>` | Leave as `none` |
| `<INITIAL_ACCESS_TOKEN>` | The long token you copied in Step 4.2 |
| `<REFRESH_TOKEN>` | Leave as `none` |
| `<JET_BOT_DEVICE_ID>` | The device ID you found in Step 4.3 |

After editing, the file should look like this (with your actual values):

```json
{
  "client_id": "none",
  "client_secret": "none",
  "access_token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "refresh_token": "none",
  "device_id": "yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy"
}
```

### Step 5.3 — Save and Close

Press **Ctrl + O**, then **Enter**, then **Ctrl + X**.

### Step 5.4 — How to Refresh the Token Every Day

Because the access token expires after 24 hours, you need to:

1. Go back to **https://account.smartthings.com/tokens**
2. Click **"Generate new token"** again (same settings as before)
3. Copy the new token
4. Open the file again: `nano ~/.tizenclaw/config/robotic_vacuum_config.json`
5. Replace the old token value with the new one
6. Save and close (Ctrl+O, Enter, Ctrl+X)
7. Restart TizenClaw: `./deploy_host.sh --restart-only`

---

## PART 6 — Start TizenClaw

### Step 6.1 — Make Sure Ollama is Running

Ollama should already be running in the background after installation.
To check, type:

```bash
ollama list
```

If you see `qwen2.5:32b` listed, Ollama is running. If you get an error,
start it with:

```bash
ollama serve &
```

### Step 6.2 — Start TizenClaw

Go into the TizenClaw folder and start it:

```bash
cd /home/gagan/strawhats/tizen_claw/tizenclaw
./deploy_host.sh
```

> **What to expect:** TizenClaw will start and show some startup messages.
> When you see a line saying the daemon is running, it is ready.

To check that it is running:

```bash
./deploy_host.sh --status
```

To view its live activity log (press **Ctrl+C** to stop watching):

```bash
./deploy_host.sh --log
```

---

## PART 7 — Test It

### Step 7.1 — Open a New Terminal Window

Press **Ctrl + Alt + T** to open a second terminal window.

### Step 7.2 — Make Sure the Command is in Your PATH

> **What PATH means:** A list of places your computer looks for programs.
> Run this once to add TizenClaw to that list:

```bash
echo 'export PATH="$HOME/.tizenclaw/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Step 7.3 — Try Talking to Your Vacuum

Type a plain-English instruction:

```bash
tizenclaw-cli ask "What is my robot vacuum's battery level?"
```

Expected reply (something like):
```json
{"status":"ok","battery_pct":85,"movement":"charging","cleaning_mode":"stop","turbo_mode":"off"}
```

```bash
tizenclaw-cli ask "Start the robot vacuum in auto mode"
```

Expected reply:
```json
{"status":"ok","action":"cleaning","mode":"auto"}
```

```bash
tizenclaw-cli ask "Send the vacuum back to its dock"
```

Expected reply:
```json
{"status":"ok","action":"homing"}
```

```bash
tizenclaw-cli ask "Set the vacuum suction to maximum"
```

Expected reply:
```json
{"status":"ok","action":"turbo","level":"on"}
```

### Step 7.4 — If Something Goes Wrong

**"connection refused" error:**
TizenClaw is not running. Go back to Step 6.2.

**"token refresh failed" or "401" error:**
Your access token has expired. Go back to Step 5.4 to get a new one.

**Vacuum does not move but you get `{"status":"ok"}`:**
The command reached Samsung's servers successfully. Check that:
- Your Jet Bot is powered on
- It is connected to Wi-Fi
- It shows as "online" in your SmartThings app

**AI seems confused or slow:**
The Ollama model may still be loading. Wait 30 seconds and try again.
Check Ollama is running: `ollama list`

---

## PART 8 — Managing TizenClaw Day-to-Day

### Start TizenClaw
```bash
cd /home/gagan/strawhats/tizen_claw/tizenclaw
./deploy_host.sh
```

### Stop TizenClaw
```bash
./deploy_host.sh --stop
```

### Restart TizenClaw (after changing any config file)
```bash
./deploy_host.sh --restart-only
```

### View live logs (activity feed)
```bash
./deploy_host.sh --log
```

---

## Commands You Can Give the Vacuum

Once TizenClaw is running, use `tizenclaw-cli ask "..."` with natural language:

| What you want | Example command |
|---|---|
| Start cleaning | `tizenclaw-cli ask "Start the vacuum"` |
| Start in a specific mode | `tizenclaw-cli ask "Start the vacuum in part cleaning mode"` |
| Pause | `tizenclaw-cli ask "Pause the vacuum"` |
| Stop | `tizenclaw-cli ask "Stop the vacuum"` |
| Go back to dock | `tizenclaw-cli ask "Dock the robot vacuum"` |
| Check status | `tizenclaw-cli ask "What is the vacuum doing?"` |
| Check battery | `tizenclaw-cli ask "How much battery does the vacuum have?"` |
| Max suction | `tizenclaw-cli ask "Set vacuum suction to maximum"` |
| Quiet mode | `tizenclaw-cli ask "Set vacuum to silent mode"` |

---

## Cleaning Mode Reference

When you say "start in part mode" or "start in map mode", here is what each means:

| Mode name | What it does |
|---|---|
| `auto` | Cleans the whole area automatically (recommended) |
| `part` | Cleans a specific spot/zone |
| `repeat` | Goes over the same area twice |
| `manual` | You control it manually |
| `map` | Maps the room without cleaning |

---

## Technical Reference (for the curious)

### How TizenClaw sends commands to the vacuum

TizenClaw talks to Samsung's SmartThings service using a web API
(a standardised way for programs to communicate over the internet).
It sends commands like:

```
POST https://api.smartthings.com/v1/devices/{YOUR_DEVICE_ID}/commands
Authorization: Bearer {YOUR_TOKEN}

{
  "commands": [{
    "component": "main",
    "capability": "robotCleanerMovement",
    "command": "setRobotCleanerMovement",
    "arguments": ["cleaning"]
  }]
}
```

Samsung responds with HTTP 204 (which simply means "done, no problems").

### SmartThings capability → action mapping

| Action | SmartThings capability | Command | Argument |
|---|---|---|---|
| Start cleaning | `robotCleanerCleaningMode` + `robotCleanerMovement` | `setRobotCleanerCleaningMode` + `setRobotCleanerMovement` | `"auto"` + `"cleaning"` |
| Stop | `robotCleanerMovement` | `setRobotCleanerMovement` | `"idle"` |
| Pause | `robotCleanerMovement` | `setRobotCleanerMovement` | `"pause"` |
| Dock | `robotCleanerMovement` | `setRobotCleanerMovement` | `"homing"` |
| Suction | `robotCleanerTurboMode` | `setRobotCleanerTurboMode` | `"on"/"off"/"silence"` |

### File locations on your Linux computer

| File | Location | Purpose |
|---|---|---|
| TizenClaw program | `~/.tizenclaw/bin/tizenclaw` | The main program |
| AI/LLM settings | `~/.tizenclaw/config/llm_config.json` | Which AI model to use |
| Vacuum credentials | `~/.tizenclaw/config/robotic_vacuum_config.json` | Your SmartThings token and device ID |
| Logs | `~/.tizenclaw/logs/` | Activity records |

### Build and deploy reference (for Tizen device/emulator)

```bash
# Build for Tizen x86_64 emulator
./deploy.sh -a x86_64 -d emulator-26101

# Fast incremental rebuild
./deploy.sh -a x86_64 -d emulator-26101 -n -i

# Skip build, redeploy existing package
./deploy.sh -a x86_64 -d emulator-26101 -s
```

---

## Source Files Created

| File | Purpose |
|---|---|
| `tools/cli/robotic-vacuum-cli/tool.md` | Tells the AI what the vacuum tool can do |
| `tools/cli/robotic-vacuum-cli/main.cc` | Entry point — reads your command and routes it |
| `tools/cli/robotic-vacuum-cli/smartthings_client.hh/.cc` | Handles HTTPS communication with Samsung |
| `tools/cli/robotic-vacuum-cli/vacuum_controller.hh/.cc` | Translates actions into SmartThings commands |
| `tools/cli/robotic-vacuum-cli/CMakeLists.txt` | Build instructions |
| `data/config/robotic_vacuum_config.json` | Your credentials template |
