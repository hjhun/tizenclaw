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
      "assets": {"type": "array", "description": "External assets [{url,filename}]"}
    },
    "required": ["app_id", "title", "html"]
  }
}
```

## Usage Tips

- For single-file apps, put everything (CSS+JS) inline in the `html` parameter
- For larger apps, use separate `css` and `js` parameters for cleaner code
- The generated app can fetch TizenClaw API endpoints: `fetch('/api/metrics')`, `fetch('/api/sessions')`, etc.
- Use `assets` to download images from external URLs: `[{"url": "https://...", "filename": "logo.png"}]`
- Asset filenames must not contain path separators or `..`
- Max asset size: 10MB per file
- Apps are managed via `/api/apps` (list) and `/api/apps/<id>` (detail/delete)
