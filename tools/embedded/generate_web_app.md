# generate_web_app

Generate a dynamic web application and serve it via the built-in web server (port 9090). Creates HTML/CSS/JS apps accessible at `http://<device-ip>:9090/apps/<app_id>/`. Use for dashboards, data visualizations, device control panels, or any interactive UI. The app can call TizenClaw REST API (same origin) for live data. External assets (images, fonts) can be downloaded automatically.

**Category**: web_app

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| app_id | string | yes | Unique app identifier (lowercase alphanumeric + underscore, max 64 chars) |
| title | string | yes | Display title for the web app |
| html | string | yes | Complete HTML content (single-file or referencing style.css/app.js) |
| css | string | no | Separate CSS stylesheet (saved as style.css) |
| js | string | no | Separate JavaScript code (saved as app.js) |
| assets | array | no | External assets to download: [{url, filename}] |
| allowed_tools | array | no | List of tool names this app can call via Bridge API (e.g. ["execute_cli"]). If omitted, no tools accessible via bridge. |

## Schema

```json
{
  "name": "generate_web_app",
  "description": "Generate a dynamic web application and serve it via the built-in web server. Creates HTML/CSS/JS apps accessible at http://<device-ip>:9090/apps/<app_id>/.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app_id": {"type": "string", "description": "Unique app identifier"},
      "title": {"type": "string", "description": "Display title"},
      "html": {"type": "string", "description": "Complete HTML content"},
      "css": {"type": "string", "description": "Optional CSS stylesheet"},
      "js": {"type": "string", "description": "Optional JavaScript code"},
      "assets": {
        "type": "array",
        "description": "External assets [{url,filename}]",
        "items": {
          "type": "object",
          "properties": {
            "url": {"type": "string"},
            "filename": {"type": "string"}
          }
        }
      },
      "allowed_tools": {"type": "array", "items": {"type": "string"}, "description": "Tool names this app can call via Bridge API"}
    },
    "required": ["app_id", "title", "html"]
  }
}
```

## Bridge API Integration

Generated web apps can call TizenClaw tools via the **Bridge API**. The `tizenclaw-sdk.js` is available at `/sdk/tizenclaw-sdk.js` and auto-detects the app_id from the URL path.

### SDK Usage

```html
<script src="/sdk/tizenclaw-sdk.js"></script>
<script>
  // One-shot tool call
  const battery = await TizenClaw.callTool('execute_cli',
      {tool_name: 'tizen-device-info-cli', arguments: 'battery'});

  // Get available tools
  const tools = await TizenClaw.getAvailableTools();

  // Ask LLM
  const answer = await TizenClaw.askLLM('Analyze battery status');
</script>
```

### Bridge API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/bridge/tool` | POST | Execute a tool: `{app_id, tool_name, arguments}` |
| `/api/bridge/tools` | GET | List available tools: `?app_id=<id>` |
| `/api/bridge/data` | GET/POST | App key-value store |
| `/api/bridge/chat` | POST | LLM chat: `{app_id, prompt}` |

## IMPORTANT: Real-Time Dashboard Pattern

When creating dashboards or monitoring apps that display live device data (battery, memory, network, display, etc.), you **MUST** use `TizenClaw.autoRefresh()` to keep the data updated automatically. Without autoRefresh, the data will only be fetched once and never update.

### autoRefresh() Usage (REQUIRED for dashboards)

```html
<script src="/sdk/tizenclaw-sdk.js"></script>
<script>
  // Auto-refresh battery info every 5 seconds
  // Returns a stop() function to cancel
  const stopBattery = TizenClaw.autoRefresh(
    'execute_cli',
    {tool_name: 'tizen-device-info-cli', arguments: 'battery'},
    function(result, err) {
      if (err) return;
      document.getElementById('battery-level').textContent = result.percent + '%';
      document.getElementById('battery-charging').textContent = result.is_charging ? 'Charging' : 'Not charging';
    },
    5000  // refresh every 5000ms (5 seconds)
  );

  // Auto-refresh display info every 3 seconds
  const stopDisplay = TizenClaw.autoRefresh(
    'execute_cli',
    {tool_name: 'tizen-display-cli', arguments: 'get'},
    function(result, err) {
      if (err) return;
      document.getElementById('brightness').textContent = result.brightness;
      document.getElementById('screen-state').textContent = result.state;
    },
    3000
  );

  // To stop refreshing (e.g. when navigating away):
  // stopBattery();
  // stopDisplay();
</script>
```

### Example: Device Status Dashboard

```html
<script src="/sdk/tizenclaw-sdk.js"></script>
<div id="battery-section">
  <h3>Battery</h3>
  <span id="bat-pct">--</span>% | <span id="bat-charge">--</span>
</div>
<div id="device-section">
  <h3>Device Info</h3>
  <pre id="dev-info">Loading...</pre>
</div>
<script>
  TizenClaw.autoRefresh('execute_cli',
    {tool_name: 'tizen-device-info-cli', arguments: 'battery'},
    function(r) {
      if (r) {
        document.getElementById('bat-pct').textContent = r.percent;
        document.getElementById('bat-charge').textContent =
          r.is_charging ? '🔌 Charging' : '🔋 On Battery';
      }
    }, 5000);

  // One-shot call for static device info
  TizenClaw.callTool('execute_cli',
    {tool_name: 'tizen-device-info-cli', arguments: 'model'})
    .then(function(r) {
      document.getElementById('dev-info').textContent =
        JSON.stringify(r, null, 2);
    });
</script>
```

## Usage Tips

- For single-file apps, put everything (CSS+JS) inline in the `html` parameter
- For larger apps, use separate `css` and `js` parameters for cleaner code
- Include `<script src="/sdk/tizenclaw-sdk.js"></script>` in HTML to use Bridge API
- Specify `allowed_tools` to grant the app access to specific device tools — use actual tool names like `execute_cli`, NOT CLI subcommand names
- **For dashboards/monitors: ALWAYS use `TizenClaw.autoRefresh()` instead of one-shot `callTool()`** so the data stays current
- The generated app can fetch TizenClaw API endpoints: `fetch('/api/metrics')`, `fetch('/api/sessions')`, etc.
- Use `assets` to download images from external URLs: `[{"url": "https://...", "filename": "logo.png"}]`
- Asset filenames must not contain path separators or `..`
- Max asset size: 10MB per file
- Apps are managed via `/api/apps` (list) and `/api/apps/<id>` (detail/delete)
