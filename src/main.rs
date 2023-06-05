use std::{eprintln, net::SocketAddr, thread, time::Duration, sync::Arc};

use anyhow::Result;
use axum::{
    body::{boxed, BoxBody, Full},
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade, Path, Query,
    },
    http::{header, Response, StatusCode, Uri},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use clap::Parser;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::sync::{broadcast, Mutex};

#[derive(clap::Parser, Clone)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The port for the web server
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

fn main() -> Result<()> {
    initialize_environment();

    let cli = Cli::parse();

    let (tx, _rx) = broadcast::channel::<String>(10000); // capacity arbitrarily chosen
    let state = AppState {
        tx: tx.clone(),
        sort_by: Arc::new(Mutex::new(None)),
        search_filter: Arc::new(Mutex::new(None))
    };

    // start web server and attempt to open it in browser
    let rt = tokio::runtime::Runtime::new()?;
    let _webserver = rt.spawn(async move {
        let app = Router::new()
            .route("/", get(root))
            .route("/search", get(search))
            .route("/tracks", get(tracks_table))
            .route("/sort/:sort_by", post(sort))
            .route("/play/:id", post(play))
            .route("/events", get(events_websocket))
            .route("/*file", get(static_handler))
            .with_state(state);

        let url = format!("http://localhost:{}", cli.port);
        let _ = open::that(url);

        let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .expect(
                "Failed to bind to socket. Maybe another service is already using the same port",
            );
    });

    loop {
        tx.send("foo".to_string())?;
        thread::sleep(Duration::from_millis(1000));
    }
}

fn initialize_environment() {
    std::env::set_var("RUST_BACKTRACE", "1");
}

#[derive(Clone)]
struct AppState {
    // TODO: replace String with whatever type you want to send to the UI
    tx: broadcast::Sender<String>,
    sort_by: Arc<Mutex<Option<String>>>,
    search_filter: Arc<Mutex<Option<String>>>
}

#[axum::debug_handler]
async fn root() -> impl IntoResponse {
    Html(include_str!("../embed/index.html"))
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    search: String,
}

#[axum::debug_handler]
async fn search(State(state): State<AppState>, Query(params): Query<SearchParams>) -> impl IntoResponse {
    state.search_filter.lock().await.replace(params.search.to_lowercase());
    tracks_html(state).await
}

#[axum::debug_handler]
async fn sort(State(state): State<AppState>, Path(sort_by): Path<String>) -> impl IntoResponse {
    state.sort_by.lock().await.replace(sort_by);
    tracks_html(state).await
}

#[axum::debug_handler]
async fn play(Path(id): Path<String>) -> impl IntoResponse {
    eprintln!("play track: {id}");
    // TODO: play track
    Html("<div id='playback-icon' hx-swap-oob='innerHTML'>▶️</div>")
}

#[axum::debug_handler]
async fn tracks_table(State(state): State<AppState>) -> impl IntoResponse {
    tracks_html(state).await
}

struct Track {
    id: i64,
    artist: String,
    album: String,
    track: String,
    seconds: i64,
}

async fn tracks_html(state: AppState) -> impl IntoResponse {
    let mut table: String = r#"<table id="tracks" hx-swap-oob='true' class="table-fixed">
    <thead class="bg-cyan-900">
      <tr>
        <th hx-post="/sort/artist" hx-trigger="click"'>Artist</th>
        <th hx-post="/sort/album" hx-trigger="click">Album</th>
        <th hx-post="/sort/track" hx-trigger="click">Track</th>
        <th class="pr-2">Length</th>
      </tr>
    </thead>
    <tbody >"#
        .into();

    // TODO what's a better way to do error handling in a handler?
    let conn = sqlite::open("chinook.db").unwrap();
    let mut sql = r#"
    with result as (
        select t.TrackId, ar.Name Artist, al.Title Album, t.Name Track, t.Milliseconds / 1000 Seconds
        from artists ar
        join albums al on ar.ArtistId = al.ArtistId
        join tracks t on al.AlbumId = t.AlbumId
    )
    select * from result"#.to_string();

    if let Some(sort_by) = state.sort_by.lock().await.as_ref() {
        sql.push_str(&format!(" order by {}", sort_by));
    }

    eprintln!("sql: {}", sql);

    let filter = state.search_filter.lock().await;

    // TODO: move search into SQL maybe? might be faster...
    let iter = conn.prepare(sql).unwrap()
    .into_iter()
    .map(|row| row.unwrap())
    .map(|row| {
        Track {
            id: row.read::<i64, _>(0),
            artist: row.read::<&str, _>(1).to_string(),
            album: row.read::<&str, _>(2).to_string(),
            track: row.read::<&str, _>(3).to_string(),
            seconds: row.read::<i64, _>(4),
        }
    })
    .filter(|track| {
        if let Some(filter) = filter.as_ref() {
            return track.artist.to_lowercase().contains(filter)
                || track.album.to_lowercase().contains(filter)
                || track.track.to_lowercase().contains(filter);
        }
        true
    });

    let start = std::time::Instant::now();
    let mut row_count = 0;
    for track in iter {
        row_count += 1;
        let len_s = track.seconds;
        table.push_str(&format!(
            r#"<tr class="even:bg-cyan-900" hx-post="/play/{}" hx-trigger="click" hx-swap="none">
        <td>{}</td>
        <td>{}</td>
        <td>{}</td>
        <td>{}</td>
      </tr>"#,
            track.id,
            track.artist,
            track.album,
            track.track,
            format!("{}:{:02}", len_s / 60, len_s % 60),
        ));
    }

    let elapsed = start.elapsed();
    eprintln!("read+rendered tracks in: {:?}", elapsed);

    // let mut rows = stmt.query([]).unwrap();


    // while let Some(row) = rows.next().unwrap() {
    //     row_count += 1;
    //     let len_s = row.get::<_, i64>(3).unwrap();
    //     let len_m = len_s / 60;
    //     let len_s = len_s % 60;
    //     let len = format!("{}:{:02}", len_m, len_s);

    //     table.push_str(&format!(
    //         r#"<tr class="even:bg-cyan-900">
    //     <td>{}</td>
    //     <td>{}</td>
    //     <td>{}</td>
    //     <td>{}</td>
    //   </tr>"#,
    //         row.get::<_, String>(0).unwrap(),
    //         row.get::<_, String>(1).unwrap(),
    //         row.get::<_, String>(2).unwrap(),
    //         len,
    //     ));
    // }

    table.push_str("</tbody></table>");

    table.push_str(&format!(
        "<div id='track-count' hx-swap-oob='true'>{row_count} tracks</div>"
    ));

    Html(table)
}

#[axum::debug_handler]
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();
    StaticFile(path)
}

#[derive(RustEmbed)]
#[folder = "embed/"]
struct Asset;

#[axum::debug_handler]
async fn events_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|ws: WebSocket| async { stream_events(state, ws).await })
}

async fn stream_events(app_state: AppState, mut ws: WebSocket) {
    // if in debug mode, tell the front-end so it can close the window when the backend dies
    #[cfg(debug_assertions)]
    {
        let _ = ws
            .send(Message::Text(
                r#"{"debug_mode": true}"#.to_string(),
            ))
            .await;
    }

    let mut rx = app_state.tx.subscribe();

    loop {
        let event = rx.recv().await.unwrap();
        // serialization is an example; don't need to do this if you're sending a string
        let serialized = serde_json::to_string(&event).unwrap();

        if let Err(e) = ws.send(Message::Text(serialized)).await {
            eprintln!("failed to send websocket message: {}", e);
            return;
        }
    }
}

pub struct StaticFile<T>(pub T);

impl<T> IntoResponse for StaticFile<T>
where
    T: Into<String>,
{
    fn into_response(self) -> Response<BoxBody> {
        let path = self.0.into();

        match Asset::get(path.as_str()) {
            Some(content) => {
                let body = boxed(Full::from(content.data));
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(body)
                    .unwrap()
            }
            None => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(boxed(Full::from("404")))
                .unwrap(),
        }
    }
}
