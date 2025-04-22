# PDF Summarizer

A web application for uploading, processing, and summarizing PDF documents. 

## Features

- PDF Upload: Easily upload PDF files through a user-friendly interface
- Text Extraction: Automatically extract text content from uploaded PDFs
- Keyword-based Summarization: Generate summaries based on word frequency analysis
- Document Management: View, manage, and delete uploaded documents
- Customizable Summaries: Adjust summary length and filtering options

## Tech Stack

- **Backend**: Rust with Axum framework
- **Frontend**: HTML, CSS, Bootstrap 5
- **PDF Processing**: pdf-extract library
- **Data Storage**: In-memory with option to persist data

## Getting Started

### Prerequisites

- Rust (latest stable version)
- Cargo package manager

### Installation

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/pdf-summarizer.git
   cd pdf-summarizer
   ```

2. Build the project:
   ```
   cargo build --release
   ```

3. Run the application:
   ```
   cargo run --release
   ```

4. Open your browser and navigate to:
   ```
   http://localhost:3000
   ```

## Project Structure

```
pdf-summarizer/
├── src/
│   └── main.rs                # Main application code
├── templates/                  # HTML templates
│   ├── index.html             # Home page template
│   ├── summary.html           # Summary display template
│   ├── history.html           # Document history template
│   └── document.html          # Document view template
├── static/                     # Static assets
├── uploads/                    # PDF storage directory
├── Cargo.toml                  # Project dependencies
└── README.md                   # This file
```

## API Endpoints

- `GET /` - Home page
- `POST /upload` - Upload a PDF file
- `GET /history` - View upload history
- `GET /view/:id` - View a specific document
- `GET /api/summary/:id` - Get document summary (JSON)
- `POST /delete/:id` - Delete a document

## Customization Options

Summaries can be customized with the following query parameters:

- `length` - Number of key terms to include (default: 20)
- `min_word_length` - Minimum word length to consider (default: 4)
- `exclude_common` - Exclude common English words (default: false)

Example: `/api/summary/123?length=10&min_word_length=5&exclude_common=true`

## Dependencies

Key dependencies include:

- `axum` - Web framework with multipart support
- `pdf-extract` - PDF text extraction
- `tokio` - Asynchronous runtime
- `serde` - Serialization/deserialization
- `tower-http` - HTTP services
- `uuid` - Unique identifier generation

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
