//! `omsi://localhost/…` (`http://omsi.localhost/…` on Windows): the content's pictures the page
//! shows - a map's `picture.jpg` from the OMSI folder, the content folder or an archive used in
//! place (`content/<path>`). Nothing else on the disk can be asked for.
//!
//! Omsi-Hub: and the bytes of our 3D view (`bytes/<cmd>?args=<json>`: the commands
//! `omsi-launcher-api`'s `omsihub::bus3d` answers with bytes - a bus's pack, a texture, a photo -
//! asked for by id, never by path), so that the page's renderer can fetch them from its worker,
//! where the window's commands are not; the phone's web view asks for them the same way
//! (`LauncherBridge.java`).

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Runtime, UriSchemeContext, UriSchemeResponder};

pub fn handle<R: Runtime>(_ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let path = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or("").to_string();
    // (reading a picture out of a 5 GB archive must not hold the window's thread)
    std::thread::spawn(move || responder.respond(if path.trim_start_matches('/').starts_with("bytes/") { bytes(&path, &query) } else { answer(&path) }));
}

/// No window behind it: the bytes asked for through the protocol need none.
struct NoWindow;

impl omsi_launcher_api::Platform for NoWindow {
    fn renderer(&self, _req: serde_json::Value, _wait: std::time::Duration) -> Result<(serde_json::Value, Vec<u8>), String> {
        Err("no renderer here".into())
    }
}

/// Omsi-Hub: `bytes/<cmd>?args=<json>` - one of our 3D view's byte commands (`b3d_…`).
fn bytes(path: &str, query: &str) -> Response<Vec<u8>> {
    let cmd = path.trim_start_matches('/').trim_start_matches("bytes/").to_string();
    let args = query.split('&').find_map(|kv| kv.strip_prefix("args=")).map(percent_decode).unwrap_or_else(|| "{}".into());
    let args: serde_json::Value = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
    let answer = if cmd.starts_with("b3d_") { omsi_launcher_api::call_bytes(&cmd, &args, &NoWindow) } else { Err("not one of these".into()) };
    match answer {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body)
            .unwrap_or_default(),
        Err(e) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(e.into_bytes())
            .unwrap_or_default(),
    }
}

fn answer(path: &str) -> Response<Vec<u8>> {
    let path = percent_decode(path.trim_start_matches('/'));
    let found = if let Some(rel) = path.strip_prefix("content/") {
        omsi_launcher_api::content_picture(rel)
    } else {
        None
    };
    match found {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime(&path))
            .header(header::CACHE_CONTROL, "max-age=300")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(bytes)
            .unwrap_or_default(),
        None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap_or_default(),
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_are_read() {
        assert_eq!(super::percent_decode("maps/Berlin%20Spandau/picture.jpg"), "maps/Berlin Spandau/picture.jpg");
        assert_eq!(super::percent_decode("a%"), "a%");
    }

    #[test]
    fn only_the_3d_view_s_bytes_are_given_and_never_a_file_by_its_path() {
        let r = super::bytes("/bytes/content_picture", "args=%7B%22path%22%3A%22envir.cfg%22%7D");
        assert_eq!(r.status(), tauri::http::StatusCode::NOT_FOUND);
        let r = super::bytes("/bytes/b3d_tex", "args=%7B%22id%22%3A%22nothing%22%7D");
        assert_eq!(r.status(), tauri::http::StatusCode::NOT_FOUND);
    }
}
