use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use uuid::Uuid;

// App configuration and state
#[derive(Clone)]
struct AppState {
    upload_dir: PathBuf,
    history: Arc<Mutex<Vec<UploadHistory>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UploadHistory {
    id: String,
    filename: String,
    timestamp: String,
    word_count: usize,
    summary: String,
}

#[derive(Debug, Deserialize)]
struct SummaryOptions {
    #[serde(default = "default_summary_length")]
    length: usize,
    #[serde(default = "default_min_word_length")]
    min_word_length: usize,
    #[serde(default)]
    exclude_common: bool,
}

fn default_summary_length() -> usize {
    20
}

fn default_min_word_length() -> usize {
    4
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup directories
    let upload_dir = PathBuf::from("uploads");
    fs::create_dir_all(&upload_dir)?;

    // Initialize state
    let state = AppState {
        upload_dir,
        history: Arc::new(Mutex::new(Vec::new())),
    };

    // Build our application with routes
    let app = Router::new()
        .route("/", get(index_page))
        .route("/upload", post(handle_upload))
        .route("/history", get(view_history))
        .route("/api/summary/:id", get(get_summary))
        .route("/view/:id", get(view_document))
        .route("/delete/:id", post(delete_document))
        .with_state(state.clone())
        .nest_service("/static", ServeDir::new("static"));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🚀 Server running on http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn index_page() -> impl IntoResponse {
    let template = include_str!("../templates/index.html");
    Html(template)
}

async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: axum::extract::multipart::Multipart,
) -> Result<impl IntoResponse, StatusError> {
    let mut file_path = None;
    let mut original_filename = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| StatusError::from_error(e))? {
        if let Some(name) = field.name() {
            if name == "file" {
                let file_name = field
                    .file_name()
                    .ok_or_else(|| StatusError::bad_request("No filename provided"))?
                    .to_string();

                if !file_name.to_lowercase().ends_with(".pdf") {
                    return Err(StatusError::bad_request("Only PDF files are accepted"));
                }

                original_filename = file_name.clone();
                let unique_id = Uuid::new_v4().to_string();
                let file_ext = Path::new(&file_name).extension().unwrap_or_default();
                let dest_path = state.upload_dir.join(format!("{}.{:?}", unique_id, file_ext));

                let data = field.bytes().await.map_err(|e| StatusError::from_error(e))?;
                if data.is_empty() {
                    return Err(StatusError::bad_request("Empty file"));
                }

                tokio::fs::write(&dest_path, &data)
                    .await
                    .map_err(|e| StatusError::from_error(e))?;

                file_path = Some((unique_id, dest_path));
            }
        }
    }

    if let Some((id, path)) = file_path {
        // Process the PDF
        let path_str = path.to_string_lossy().to_string();
        let text = match pdf_extract::extract_text(&path_str) {
            Ok(text) => text,
            Err(e) => {
                // Clean up failed file
                let _ = tokio::fs::remove_file(path).await;
                return Err(StatusError::internal_server_error(format!("Failed to extract text: {}", e)));
            }
        };

        // Generate summary
        let options = SummaryOptions {
            length: default_summary_length(),
            min_word_length: default_min_word_length(),
            exclude_common: true,
        };

        let summary = advanced_summarize(&text, &options);
        let word_count = text.split_whitespace().count();

        // Save to history
        let history_entry = UploadHistory {
            id: id.clone(),
            filename: original_filename,
            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            word_count,
            summary: summary.clone(),
        };

        let mut history = state.history.lock().await;
        history.push(history_entry);

        // Prepare response using the template
        let template = include_str!("../templates/summary.html");
        let html = template
            .replace("{id}", &id)
            .replace("{summary}", &summary)
            .replace("{word_count}", &word_count.to_string());

        Ok(Html(html))
    } else {
        Err(StatusError::bad_request("No file was uploaded"))
    }
}

async fn view_history(State(state): State<AppState>) -> impl IntoResponse {
    let history = state.history.lock().await;

    let mut history_items = String::new();
    for entry in history.iter() {
        history_items.push_str(&format!(
            r#"<tr>
                <td>{}</td>
                <td>{}</td>
                <td>{} words</td>
                <td>
                    <a href="/view/{}" class="btn btn-sm btn-primary">View</a>
                    <button class="btn btn-sm btn-danger" onclick="deleteDocument('{}')">Delete</button>
                </td>
            </tr>"#,
            entry.filename, entry.timestamp, entry.word_count, entry.id, entry.id
        ));
    }

    // Get the template and replace the placeholder
    let template = include_str!("../templates/history.html");
    let html = template.replace("{history_items}", &history_items);

    Html(html)
}

async fn view_document(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StatusError> {
    let history = state.history.lock().await;

    if let Some(entry) = history.iter().find(|e| e.id == id) {
        let file_path = state.upload_dir.join(format!("{}.pdf", id));
        if !file_path.exists() {
            return Err(StatusError::not_found("PDF file not found"));
        }

        let text = pdf_extract::extract_text(&*file_path.to_string_lossy())
            .map_err(|e| StatusError::internal_server_error(format!("Failed to extract text: {}", e)))?;

        let word_count = text.split_whitespace().count();
        let paragraphs = text
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .map(|p| format!("<p>{}</p>", p))
            .collect::<Vec<_>>()
            .join("");

        // Get the template and replace all placeholders
        let template = include_str!("../templates/document.html");
        let html = template
            .replace("{id}", &id)
            .replace("{filename}", &entry.filename)
            .replace("{timestamp}", &entry.timestamp)
            .replace("{word_count}", &word_count.to_string())
            .replace("{summary}", &entry.summary)
            .replace("{content}", &paragraphs);

        Ok(Html(html))
    } else {
        Err(StatusError::not_found("Document not found"))
    }
}

async fn get_summary(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(options): Query<SummaryOptions>,
) -> Result<impl IntoResponse, StatusError> {
    let history = state.history.lock().await;

    if let Some(entry) = history.iter().find(|e| e.id == id) {
        let file_path = state.upload_dir.join(format!("{}.pdf", id));
        if !file_path.exists() {
            return Err(StatusError::not_found("PDF file not found"));
        }

        let text = pdf_extract::extract_text(&*file_path.to_string_lossy())
            .map_err(|e| StatusError::internal_server_error(format!("Failed to extract text: {}", e)))?;

        let summary = advanced_summarize(&text, &options);

        Ok(Json(ApiResponse {
            success: true,
            message: "Summary generated".to_string(),
            data: Some(serde_json::json!({
                "id": id,
                "filename": entry.filename,
                "timestamp": entry.timestamp,
                "word_count": text.split_whitespace().count(),
                "summary": summary,
            })),
        }))
    } else {
        Err(StatusError::not_found("Document not found"))
    }
}

async fn delete_document(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StatusError> {
    let mut history = state.history.lock().await;

    let position = history
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| StatusError::not_found("Document not found"))?;

    // Remove from history
    history.remove(position);

    // Delete the file
    let file_path = state.upload_dir.join(format!("{}.pdf", id));
    if file_path.exists() {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| StatusError::internal_server_error(format!("Failed to delete file: {}", e)))?;
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Document deleted successfully".to_string(),
        data: None,
    }))
}

fn advanced_summarize(text: &str, options: &SummaryOptions) -> String {
    // Common English stopwords to filter out if exclude_common is true
    let stopwords: Vec<&str> = vec![
        "the", "and", "a", "to", "of", "in", "is", "it", "you", "that", "he", "was", "for", "on",
        "are", "with", "as", "this", "that", "from", "have", "been", "has", "had", "not", "what",
        "all", "were", "when", "we", "there", "can", "an", "which", "their", "said", "if", "will",
        "would", "about", "them", "then", "she", "many", "these", "so", "some", "her", "like",
        "him", "into", "time", "could", "no", "make", "than", "first", "been", "its", "who", "now",
    ];

    // Tokenize and normalize text
    let words = text
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| w.len() >= options.min_word_length)
        .filter(|w| !options.exclude_common || !stopwords.contains(&w.as_str()))
        .collect::<Vec<_>>();

    // Calculate word frequencies
    let mut freq = HashMap::new();
    for word in &words {
        *freq.entry(word).or_insert(0) += 1;
    }

    // Extract keywords
    let mut common_words = freq.iter().collect::<Vec<_>>();
    common_words.sort_by(|a, b| b.1.cmp(a.1));

    // Get top words
    let top_words = common_words.iter().take(options.length).map(|(w, _)| *w).collect::<Vec<_>>();

    if top_words.is_empty() {
        return "No significant content found".to_string();
    }

    // Handle simple frequency-based summary
    let summary = common_words
        .iter()
        .take(options.length)
        .map(|(word, count)| format!("{} ({})", word, count))
        .collect::<Vec<_>>()
        .join(", ");

    summary
}

// Custom error handling
struct StatusError {
    status: StatusCode,
    message: String,
}

impl StatusError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal_server_error(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn from_error<E: std::fmt::Display>(err: E) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Internal error: {}", err),
        }
    }
}

impl IntoResponse for StatusError {
    fn into_response(self) -> Response {
        let html = format!(
            r#"
            <html>
            <head>
                <title>Error - PDF Summarizer</title>
                <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.1.3/dist/css/bootstrap.min.css" rel="stylesheet">
            </head>
            <body>
                <div class="container mt-5">
                    <div class="alert alert-danger">
                        <h4>Error {}</h4>
                        <p>{}</p>
                        <a href="/" class="btn btn-primary">Back to Home</a>
                    </div>
                </div>
            </body>
            </html>
            "#,
            self.status.as_u16(),
            self.message
        );

        (self.status, Html(html)).into_response()
    }
}