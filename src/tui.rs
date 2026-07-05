//! `alberto tui` — navegador interactivo de NodeService en la terminal.
//!
//! * Panel izquierdo: árbol de nodos (doclib → tipos → carpetas → documentos),
//!   navegable con ↑↓/Enter/Backspace.
//! * Panel derecho: detalle JSON del nodo seleccionado, o **preview del PDF**.
//! * Preview: descarga el contenido por gRPC (`NodeContent`), rasteriza la
//!   página con `pdftoppm` (poppler) y la pinta con半bloques `▀` en RGB —
//!   funciona en cualquier terminal truecolor, sin protocolos gráficos.
//!
//! Teclas: ↑↓ mover · Enter entrar/preview · Backspace subir · p preview ·
//! d descargar · ←→ páginas del PDF · Esc cerrar preview · q salir.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use image::RgbaImage;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget, Wrap};
use ratatui::Terminal;
use serde_json::Value;
use tonic::transport::Channel;

use crate::cli::GrpcOpts;
use crate::client::nodemanager::node_manager_service_client::NodeManagerServiceClient;
use crate::client::{nm_client, nodemanager, with_key};

// ---------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------

struct Level {
    title: String,
    parent_id: String,
    nodes: Vec<Value>,
    state: ListState,
}

struct Preview {
    name: String,
    pdf_path: PathBuf,
    _dir: tempfile::TempDir,
    total_pages: usize,
    page: usize,
    img: RgbaImage,
}

struct App {
    client: NodeManagerServiceClient<Channel>,
    api_key: String,
    levels: Vec<Level>,
    preview: Option<Preview>,
    status: String,
}

// ---------------------------------------------------------------------------
// Entrada
// ---------------------------------------------------------------------------

pub fn run(tenant: String, grpc: GrpcOpts) -> Result<()> {
    let (mut client, conn) = block_on(nm_client(&grpc))?;
    let api_key = conn.api_key;

    // nivel raíz: hijos del document library del tenant
    let doclib = block_on(fetch_doclib(&mut client, &api_key, &tenant))?;
    let doclib_id = doclib["unique_id"].as_str().unwrap_or_default().to_string();
    let nodes = block_on(fetch_children(&mut client, &api_key, &doclib_id))?;

    let mut app = App {
        client,
        api_key,
        levels: vec![level(format!("doclib {tenant}"), doclib_id.clone(), nodes)],
        preview: None,
        status:
            "↑↓ mover · Enter entrar/preview · Backspace subir · p preview · d descargar · r refrescar · q salir"
                .into(),
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn level(title: String, parent_id: String, nodes: Vec<Value>) -> Level {
    let mut state = ListState::default();
    if !nodes.is_empty() {
        state.select(Some(0));
    }
    Level {
        title,
        parent_id,
        nodes,
        state,
    }
}

// ---------------------------------------------------------------------------
// Loop principal
// ---------------------------------------------------------------------------

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // modo preview: navegación de páginas
            if app.preview.is_some() {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => app.preview = None,
                    KeyCode::Left => change_page(app, -1),
                    KeyCode::Right => change_page(app, 1),
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Up => move_sel(app, -1),
                KeyCode::Down => move_sel(app, 1),
                KeyCode::Backspace | KeyCode::Left => {
                    if app.levels.len() > 1 {
                        app.levels.pop();
                    }
                }
                KeyCode::Enter | KeyCode::Right => enter(app),
                KeyCode::Char('p') => open_preview(app),
                KeyCode::Char('d') => download_selected(app),
                KeyCode::Char('r') => refresh(app),
                _ => {}
            }
        }
    }
}

fn current(app: &App) -> Option<&Value> {
    let lvl = app.levels.last()?;
    lvl.nodes.get(lvl.state.selected()?)
}

fn move_sel(app: &mut App, delta: i32) {
    if let Some(lvl) = app.levels.last_mut() {
        let n = lvl.nodes.len();
        if n == 0 {
            return;
        }
        let cur = lvl.state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(n as i32) as usize;
        lvl.state.select(Some(next));
    }
}

fn enter(app: &mut App) {
    let Some(node) = current(app).cloned() else {
        return;
    };
    let has_content = node["content"].as_bool().unwrap_or(false);

    if has_content {
        open_preview(app);
        return;
    }

    let id = node["unique_id"].as_str().unwrap_or_default().to_string();
    let name = node["name"].as_str().unwrap_or("?").to_string();

    match block_on(fetch_children(&mut app.client, &app.api_key, &id)) {
        Ok(nodes) if nodes.is_empty() => app.status = format!("{name}: sin hijos"),
        Ok(nodes) => {
            app.status = format!("{name}: {} hijos", nodes.len());
            app.levels.push(level(name, id, nodes));
        }
        Err(e) => app.status = format!("error: {e:#}"),
    }
}

fn refresh(app: &mut App) {
    let Some(parent_id) = app.levels.last().map(|l| l.parent_id.clone()) else {
        return;
    };
    let fetched = block_on(fetch_children(&mut app.client, &app.api_key, &parent_id));
    let lvl = app.levels.last_mut().unwrap();
    match fetched {
        Ok(nodes) => {
            let sel = lvl
                .state
                .selected()
                .unwrap_or(0)
                .min(nodes.len().saturating_sub(1));
            lvl.state
                .select(if nodes.is_empty() { None } else { Some(sel) });
            app.status = format!("refrescado: {} elementos", nodes.len());
            lvl.nodes = nodes;
        }
        Err(e) => app.status = format!("refresh: {e:#}"),
    }
}

// ---------------------------------------------------------------------------
// Preview de PDF
// ---------------------------------------------------------------------------

fn open_preview(app: &mut App) {
    let Some(node) = current(app).cloned() else {
        return;
    };

    if !node["content"].as_bool().unwrap_or(false) {
        app.status = "el nodo no tiene contenido".into();
        return;
    }

    let id = node["unique_id"].as_str().unwrap_or_default().to_string();
    let name = node["name"].as_str().unwrap_or("documento").to_string();
    app.status = format!("descargando {name}...");

    match block_on(fetch_content(&mut app.client, &app.api_key, &id))
        .and_then(|bytes| build_preview(&name, bytes))
    {
        Ok(p) => {
            app.status = format!(
                "{name} — página 1/{} · ←→ páginas · Esc cerrar",
                p.total_pages
            );
            app.preview = Some(p);
        }
        Err(e) => app.status = format!("preview: {e:#}"),
    }
}

fn build_preview(name: &str, bytes: Vec<u8>) -> Result<Preview> {
    if !bytes.starts_with(b"%PDF") {
        bail!("solo hay preview para PDFs (el contenido no es PDF)");
    }

    let dir = tempfile::tempdir().context("no se pudo crear tmpdir")?;
    let pdf_path = dir.path().join("doc.pdf");
    std::fs::write(&pdf_path, &bytes)?;

    let total_pages = pdf_pages(&pdf_path).unwrap_or(1);
    let img = render_page(&pdf_path, 1, dir.path())?;

    Ok(Preview {
        name: name.to_string(),
        pdf_path,
        _dir: dir,
        total_pages,
        page: 1,
        img,
    })
}

fn change_page(app: &mut App, delta: i32) {
    let Some(p) = app.preview.as_mut() else {
        return;
    };
    let next = p.page as i32 + delta;
    if next < 1 || next > p.total_pages as i32 {
        return;
    }

    let tmp = p.pdf_path.parent().unwrap().to_path_buf();
    match render_page(&p.pdf_path, next as usize, &tmp) {
        Ok(img) => {
            p.page = next as usize;
            p.img = img;
            app.status = format!(
                "{} — página {}/{} · ←→ páginas · Esc cerrar",
                p.name, p.page, p.total_pages
            );
        }
        Err(e) => app.status = format!("página: {e:#}"),
    }
}

fn pdf_pages(pdf: &Path) -> Option<usize> {
    let out = Command::new("pdfinfo").arg(pdf).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.starts_with("Pages:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse().ok())
}

fn render_page(pdf: &Path, page: usize, dir: &Path) -> Result<RgbaImage> {
    let prefix = dir.join("page");
    let status = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            "110",
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
        ])
        .arg(pdf)
        .arg(&prefix)
        .status()
        .context("pdftoppm no está instalado (paquete poppler / poppler-utils)")?;

    if !status.success() {
        bail!("pdftoppm falló en la página {page}");
    }

    // pdftoppm agrega sufijo -N / -0N según el total de páginas: buscarlo
    let png = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "png"))
        .context("pdftoppm no generó la imagen")?;

    let img = image::open(&png)
        .context("no se pudo leer el PNG")?
        .to_rgba8();
    let _ = std::fs::remove_file(&png);
    Ok(img)
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(outer[0]);

    draw_list(f, app, panes[0]);

    if app.preview.is_some() {
        draw_preview(f, app, panes[1]);
    } else {
        draw_detail(f, app, panes[1]);
    }

    let status = Paragraph::new(Line::from(app.status.clone()))
        .style(Style::default().fg(Color::Black).bg(Color::Cyan));
    f.render_widget(status, outer[1]);
}

fn draw_list(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let breadcrumb = app
        .levels
        .iter()
        .map(|l| l.title.as_str())
        .collect::<Vec<_>>()
        .join(" / ");

    let lvl = app.levels.last_mut().unwrap();

    let items: Vec<ListItem> = lvl
        .nodes
        .iter()
        .map(|n| {
            let name = n["name"].as_str().unwrap_or("?");
            let has_content = n["content"].as_bool().unwrap_or(false);
            let icon = if has_content { "📄" } else { "📁" };
            ListItem::new(format!("{icon} {name}"))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {breadcrumb} ")),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut lvl.state);
}

fn draw_detail(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let text = match current(app) {
        Some(node) => serde_json::to_string_pretty(node).unwrap_or_default(),
        None => "(vacío)".into(),
    };

    let detail = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" detalle "));
    f.render_widget(detail, area);
}

fn draw_preview(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let p = app.preview.as_ref().unwrap();
    let block = Block::default().borders(Borders::ALL).title(format!(
        " 📄 {} — pág {}/{} ",
        p.name, p.page, p.total_pages
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(HalfblockImage(&p.img), inner);
}

/// Pinta una imagen en celdas de terminal usando '▀': cada celda son 2 píxeles
/// verticales (fg = píxel superior, bg = inferior). Truecolor requerido.
struct HalfblockImage<'a>(&'a RgbaImage);

impl Widget for HalfblockImage<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (iw, ih) = self.0.dimensions();
        let (tw, th) = (area.width as u32, area.height as u32 * 2);

        // escala preservando proporción
        let scale = (tw as f64 / iw as f64).min(th as f64 / ih as f64);
        let (rw, rh) = (
            ((iw as f64 * scale) as u32).max(1),
            ((ih as f64 * scale) as u32).max(1),
        );

        let img = image::imageops::resize(self.0, rw, rh, image::imageops::FilterType::Triangle);
        let x0 = area.x + ((tw - rw) / 2) as u16;

        for cy in 0..(rh.div_ceil(2)) {
            for cx in 0..rw {
                let top = img.get_pixel(cx, cy * 2);
                let bot_y = cy * 2 + 1;
                let bot = if bot_y < rh {
                    *img.get_pixel(cx, bot_y)
                } else {
                    *top
                };

                let (sx, sy) = (x0 + cx as u16, area.y + cy as u16);
                if sx >= area.x + area.width || sy >= area.y + area.height {
                    continue;
                }

                if let Some(cell) = buf.cell_mut((sx, sy)) {
                    cell.set_char('▀')
                        .set_fg(Color::Rgb(top[0], top[1], top[2]))
                        .set_bg(Color::Rgb(bot[0], bot[1], bot[2]));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fetch gRPC (bloqueante sobre el runtime tokio)
// ---------------------------------------------------------------------------

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
}

fn parse_list(json: &str) -> Result<Vec<Value>> {
    match serde_json::from_str::<Value>(json)? {
        Value::Array(a) => Ok(a),
        other => Ok(vec![other]),
    }
}

async fn fetch_doclib(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    tenant: &str,
) -> Result<Value> {
    let req = nodemanager::TenantRequest {
        tenant: tenant.to_string(),
    };
    let reply = c.doc_lib(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("doclib de '{tenant}': {}", reply.error);
    }
    Ok(serde_json::from_str(&reply.result_json)?)
}

async fn fetch_children(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    unique_id: &str,
) -> Result<Vec<Value>> {
    let req = nodemanager::NodeChildRequest {
        unique_id: unique_id.to_string(),
        secondary: false,
    };
    let reply = c.node_child(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("{}", reply.error);
    }
    parse_list(&reply.result_json)
}

async fn fetch_content(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    unique_id: &str,
) -> Result<Vec<u8>> {
    let req = nodemanager::UniqueIdRequest {
        unique_id: unique_id.to_string(),
    };
    let reply = c.node_content(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("{}", reply.error);
    }
    Ok(reply.content)
}

/// Nombre seguro para escribir en el directorio actual: sin separadores de
/// ruta y sin puntos iniciales (nada de ../, rutas absolutas ni ocultos).
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "descarga.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn download_selected(app: &mut App) {
    let Some(node) = current(app).cloned() else {
        return;
    };

    if !node["content"].as_bool().unwrap_or(false) {
        app.status = "el nodo no tiene contenido".into();
        return;
    }

    let id = node["unique_id"].as_str().unwrap_or_default().to_string();
    let name = sanitize_filename(node["name"].as_str().unwrap_or("descarga.bin"));

    match block_on(fetch_content(&mut app.client, &app.api_key, &id)) {
        Ok(bytes) => {
            let size = bytes.len();
            let dest = std::env::current_dir().unwrap_or_default().join(&name);
            match std::fs::write(&dest, bytes) {
                Ok(()) => app.status = format!("descargado {} ({size} bytes)", dest.display()),
                Err(e) => app.status = format!("error escribiendo {}: {e}", dest.display()),
            }
        }
        Err(e) => app.status = format!("download: {e:#}"),
    }
}

// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_de_pdf_real() {
        let pdf = std::env::var("TEST_PDF").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/one-page.pdf").to_string()
        });
        let bytes = std::fs::read(pdf).unwrap();
        let p = build_preview("test.pdf", bytes).expect("build_preview");
        assert_eq!(p.total_pages, 1);
        assert!(p.img.width() > 100 && p.img.height() > 100);
    }

    #[test]
    fn rechaza_no_pdf() {
        assert!(build_preview("x.bin", b"no soy pdf".to_vec()).is_err());
    }

    #[test]
    fn sanitize_quita_separadores_y_puntos_iniciales() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_filename("informe 2026.pdf"), "informe 2026.pdf");
        assert_eq!(sanitize_filename(""), "descarga.bin");
        assert_eq!(sanitize_filename("..."), "descarga.bin");
        assert_eq!(sanitize_filename("a\\b/c"), "a_b_c");
    }
}
