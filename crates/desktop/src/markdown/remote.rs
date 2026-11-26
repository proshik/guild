//! Remote images belong only to the Markdown document cache, not the app client.
use gpui::{App, Image, ImageCacheError, ImageFormat, RenderImage, Task, Window};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

type Loaded = Result<Arc<RenderImage>, ImageCacheError>;
pub(super) struct Remote {
    result: Arc<Mutex<Option<Loaded>>>,
    _task: Task<()>,
    generation: u64,
}
impl Remote {
    pub(super) fn start(url: String, window: &mut Window, cx: &mut App) -> Self {
        let generation = crate::network::generation();
        let network = crate::network::snapshot();
        let result = Arc::new(Mutex::new(None));
        let loaded = result.clone();
        let renderer = cx.svg_renderer();
        let request = cx.background_executor().spawn(async move {
            fetch_with_policy(&url, &network)
                .and_then(|image| image.to_image_data(renderer))
                .map_err(|_| ImageCacheError::Asset("Markdown image could not be loaded".into()))
        });
        let entity = window.current_view();
        let task = window.spawn(cx, async move |cx| {
            *loaded.lock().unwrap() = Some(request.await);
            cx.on_next_frame(move |_, cx| cx.notify(entity));
        });
        Self {
            result,
            _task: task,
            generation,
        }
    }
    pub(super) fn retry(&self) -> bool {
        self.generation != crate::network::generation() && matches!(self.get(), Some(Err(_)))
    }
    pub(super) fn get(&self) -> Option<Loaded> {
        self.result.lock().unwrap().clone()
    }
}

#[cfg(test)]
fn fetch(address: &str) -> anyhow::Result<Image> {
    fetch_with_policy(address, &Default::default())
}
fn fetch_with_policy(
    address: &str,
    network: &guild_session::network::Network,
) -> anyhow::Result<Image> {
    let url = gpui::http_client::Url::parse(address)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https"),
        "Unsupported image scheme"
    );
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Image credentials are not supported"
    );
    let config = ureq::Agent::config_builder()
        .proxy(crate::network::http_policy(network)?)
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(5)
        .build();
    let agent = crate::network::http_agent(config, network)?;
    let mut response = agent.get(url.as_str()).call()?;
    let format = response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .and_then(|mime| ImageFormat::from_mime_type(mime.split(';').next().unwrap_or("").trim()));
    let bytes = response
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_vec()?;
    let format = format.unwrap_or_else(|| {
        if bytes.starts_with(b"\x89PNG") {
            ImageFormat::Png
        } else if bytes.starts_with(b"\xff\xd8") {
            ImageFormat::Jpeg
        } else if bytes.starts_with(b"GIF8") {
            ImageFormat::Gif
        } else if bytes.starts_with(b"RIFF") {
            ImageFormat::Webp
        } else if bytes.starts_with(b"BM") {
            ImageFormat::Bmp
        } else {
            ImageFormat::Svg
        }
    });
    Ok(Image::from_bytes(format, bytes))
}

#[cfg(test)]
mod tests {
    // Private transport: no public API is added just to reach the fetcher.
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn downloads_image_bytes_and_rejects_oversized_responses() {
        for oversized in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/image.svg", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = socket.read(&mut request);
                let body = include_bytes!("../../tests/fixtures/markdown-preview.svg");
                let len = if oversized {
                    8 * 1024 * 1024 + 1
                } else {
                    body.len()
                };
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n").unwrap();
                if oversized {
                    // A complete response would succeed without the size bound.
                    // The bounded client may close early, so BrokenPipe is expected.
                    let chunk = [b'x'; 64 * 1024];
                    let mut remaining = len;
                    while remaining > 0 {
                        let count = remaining.min(chunk.len());
                        if socket.write_all(&chunk[..count]).is_err() {
                            break;
                        }
                        remaining -= count;
                    }
                } else {
                    socket.write_all(body).unwrap();
                }
            });
            let image = fetch(&url);
            if oversized {
                let error = image.unwrap_err().to_string();
                assert!(
                    error.contains("limit"),
                    "expected response limit error, got {error}"
                );
            } else {
                assert_eq!(
                    image.unwrap().bytes,
                    include_bytes!("../../tests/fixtures/markdown-preview.svg")
                );
            }
            server.join().unwrap();
        }
    }
}
