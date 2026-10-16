use axum::response::Html;

const INDEX_HTML: &str = include_str!("../../assets/web/index.html");

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}
