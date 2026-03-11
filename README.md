# onething-monitor

Rust monitoring bot for [OneThingCloud (网心云)](https://www.onethingcloud.com/) consolepro platform. Monitors device status, network line health, earnings, and recruit business status, and sends alerts via Telegram.

## Features

- **Device Status Monitoring** — Detects online/offline/error transitions (60s interval)
- **Network Line Monitoring** — Tracks per-device line status: offline lines, high packet loss, high latency (5min interval)
- **Income Monitoring** — Alerts on zero income or significant drops (5min interval)
- **Recruit Status Monitoring** — Tracks recruit device status changes (5min interval)
- **Daily Report** — Sends a daily income summary at a configurable hour
- **Startup Summary** — Reports current status on startup
- **State Persistence** — Saves state to `state.json` to avoid duplicate alerts after restart

## Quick Start

### Prerequisites

- Rust 1.70+ (or Docker)
- A Telegram bot token (from [@BotFather](https://t.me/BotFather))
- OneThingCloud consolepro account cookies

### Build & Run

```bash
# Clone
git clone https://github.com/your-username/onething-monitor.git
cd onething-monitor

# Configure
cp config.example.toml config.toml
# Edit config.toml with your credentials (see Configuration below)

# Build and run
cargo run --release
```

### Docker

```bash
# Build
docker build -t onething-monitor .

# Run
docker run -d \
  --name onething-monitor \
  --restart unless-stopped \
  -v $(pwd)/config.toml:/app/config.toml:ro \
  -v $(pwd)/state.json:/app/state.json \
  onething-monitor
```

Or with Docker Compose:

```yaml
services:
  onething-monitor:
    build: .
    restart: unless-stopped
    volumes:
      - ./config.toml:/app/config.toml:ro
      - ./state.json:/app/state.json
```

## Configuration

Copy `config.example.toml` to `config.toml` and fill in your credentials:

```toml
[api]
# From browser cookies after logging in to consolepro.onethingcloud.com
# Open DevTools -> Application -> Cookies to find these values
session_id = "your_session_id"
user_id = "your_user_id"

[telegram]
bot_token = "123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
chat_id = "your_chat_id"

[monitor]
device_check_interval_secs = 60   # Device status check interval
income_check_interval_secs = 300  # Income/recruit/line check interval
daily_report_hour = 9             # Hour to send daily report (0-23, local time)
log_level = "info"                # trace, debug, info, warn, error

[alert]
income_drop_threshold = 0.5       # Alert if income drops > 50%
notify_on_recovery = true         # Notify when devices recover
```

### Getting Credentials

**OneThingCloud cookies:**
1. Log in to [consolepro.onethingcloud.com](https://consolepro.onethingcloud.com)
2. Open browser DevTools (F12) -> Application -> Cookies
3. Copy `sessionid` and `userid` values

**Telegram bot:**
1. Message [@BotFather](https://t.me/BotFather) to create a bot and get the token
2. Message your bot, then call `https://api.telegram.org/bot<TOKEN>/getUpdates` to find your `chat_id`

## Alert Examples

```
# Device went offline
🔴 my-device 离线

# Network line issues
🔌 my-device 离线线路: 0 → 3
  · account1 (192.168.1.1) eth0 - 未连接
  · account2 (192.168.1.2) eth1 - 未连接

# Income drop
📉 my-device (x86) 收益大幅下降: 100.0 -> 30.0 (下降70%)

# Recovery
✅ my-device 离线线路已全部恢复 (之前:3)
```

## Project Structure

```
src/
├── main.rs              # Entry point, two async monitoring loops
├── config.rs            # TOML config loading
├── state.rs             # JSON file state persistence
├── api/
│   ├── client.rs        # HTTP client with cookie auth
│   ├── types.rs         # API request/response types
│   ├── device.rs        # Device & line data APIs
│   ├── recruit.rs       # Recruit device API
│   └── proxy.rs         # Day/month bills API
├── monitor/
│   ├── device_monitor.rs   # Device status change detection
│   ├── income_monitor.rs   # Income change detection
│   ├── line_monitor.rs     # Network line status monitoring
│   ├── recruit_monitor.rs  # Recruit status change detection
│   └── alert_monitor.rs    # Alert formatting & reports
└── notify/
    └── telegram.rs      # Telegram Bot messaging
```

## License

MIT
