//! Terminal UI for the ledger node.
use std::{
    io,
    time::{Duration, Instant},
};

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Base URL of the running ledger-node (e.g. http://127.0.0.1:3000)
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    node: String,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Dashboard,
    Chain,
    Mempool,
    Mine,
    HashDemo,
}

#[derive(Debug, Clone, Deserialize)]
struct Head {
    height: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct Tip {
    height: u64,
    hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BlockRow {
    index: u64,
    ts: u64,
    nonce: u64,
    hash: String,
    previous_hash: String,
    merkle_root: String,
    data_hash: String,
    tx_count: usize,
    data: String,
}

#[derive(Debug, Clone, Serialize)]
struct TxIn {
    from: String,
    to: String,
    amount: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TxRow {
    from: String,
    to: String,
    amount: u64,
    timestamp: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct MineResult {
    mined: bool,
    index: Option<u64>,
    nonce: Option<u64>,
    hash: Option<String>,
    target: Option<u32>,
    error: Option<String>,
}

#[derive(Debug)]
struct App {
    args: Args,
    http: Client,
    tab: Tab,
    // dashboard
    head: Option<Head>,
    tip: Option<Tip>,
    last_refresh: Instant,
    // chain list
    chain_rows: Vec<BlockRow>,
    chain_cursor: usize,
    chain_state: TableState,
    chain_scroll: ScrollbarState,
    // mempool tx list
    tx_rows: Vec<TxRow>,
    tx_cursor: usize,
    tx_state: TableState,
    tx_scroll: ScrollbarState,
    // mempool/tx form
    tx_from: String,
    tx_to: String,
    tx_amount: String,
    tx_status: Option<String>,
    //
    // mining
    mine_target: u32,
    mine_data: String,
    mine_status: Option<String>,
    // hash demo
    hash_input: String,
    hash_output: String,
    hash_leading_zeros: u32,
}

// Each item in the chain & mempool tables is 1 row high
const ITEM_HEIGHT: usize = 1;

impl App {
    fn new(args: Args) -> Self {
        Self {
            args,
            http: Client::new(),
            tab: Tab::Dashboard,
            head: None,
            tip: None,
            last_refresh: Instant::now(),
            chain_rows: Vec::new(),
            chain_cursor: 0,
            chain_state: TableState::default(),
            chain_scroll: ScrollbarState::default(),
            tx_rows: Vec::new(),
            tx_cursor: 0,
            tx_state: TableState::default(),
            tx_scroll: ScrollbarState::default(),
            tx_from: "alice".into(),
            tx_to: "bob".into(),
            tx_amount: "42".into(),
            tx_status: None,
            mine_target: 20,
            mine_data: String::new(),
            mine_status: None,
            hash_input: String::new(),
            hash_output: String::new(),
            hash_leading_zeros: 0,
        }
    }

    async fn refresh_dashboard(&mut self) {
        let base = &self.args.node;
        if let Ok(resp) = self
            .http
            .get(format!("{base}/chain/head"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(head) = resp.json::<Head>().await {
                self.head = Some(head);
            }
        }
        if let Ok(resp) = self
            .http
            .get(format!("{base}/chain/tip"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(tip) = resp.json::<Tip>().await {
                self.tip = Some(tip);
            }
        }
        self.last_refresh = Instant::now();
    }

    async fn load_chain_page(&mut self, start: Option<u64>, limit: u32, desc: bool) {
        let base = &self.args.node;
        let dir = if desc { "desc" } else { "asc" };
        let mut url = format!("{base}/chain/blocks?limit={limit}&dir={dir}");
        if let Some(s) = start {
            url.push_str(&format!("&start={s}"));
        }
        match self
            .http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.json::<Vec<BlockRow>>().await {
                Ok(rows) => {
                    self.chain_rows = rows;
                    self.chain_cursor = 0;
                }
                Err(e) => {
                    self.chain_rows.clear();
                    self.status_message = Some(format!("Failed to decode blocks: {e}"));
                }
            },
            Err(e) => {
                self.chain_rows.clear();
                self.chain_cursor = 0;
                self.status_message = Some(format!("Failed to load blocks: {e}"));
            }
        }
    }

    async fn next_row(&mut self) {
        match self.tab {
            Tab::Mempool => {
                let i = match self.tx_state.selected() {
                    Some(i) => {
                        if i >= self.tx_rows.len() - 1 {
                            self.tx_cursor = 0;
                            0
                        } else {
                            self.tx_cursor += 1;
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.tx_state.select(Some(i));
                self.tx_scroll = self.tx_scroll.position(i * ITEM_HEIGHT);
            }
            Tab::Chain => {
                let i = match self.chain_state.selected() {
                    Some(i) => {
                        if i >= self.chain_rows.len() - 1 {
                            self.chain_cursor = 0;
                            0
                        } else {
                            self.chain_cursor += 1;
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.chain_state.select(Some(i));
                self.chain_scroll = self.chain_scroll.position(i * ITEM_HEIGHT);
            }
            _ => {}
        }
    }

    async fn previous_row(&mut self) {
        match self.tab {
            Tab::Mempool => {
                let i = match self.tx_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.tx_cursor = self.tx_rows.len() - 1;
                            self.tx_rows.len() - 1
                        } else {
                            self.tx_cursor -= 1;
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.tx_state.select(Some(i));
                self.tx_scroll = self.tx_scroll.position(i * ITEM_HEIGHT);
            }
            Tab::Chain => {
                let i = match self.chain_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.chain_cursor = self.chain_rows.len() - 1;
                            self.chain_rows.len() - 1
                        } else {
                            self.chain_cursor -= 1;
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.chain_state.select(Some(i));
                self.chain_scroll = self.chain_scroll.position(i * ITEM_HEIGHT);
            }
            _ => {}
        }
    }

    async fn load_mempool_page(&mut self) {
        let base = &self.args.node;
        let url = format!("{base}/mempool");

        match self
            .http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.json::<Vec<TxRow>>().await {
                Ok(rows) => {
                    self.tx_rows = rows;
                    self.tx_cursor = 0;
                }
                Err(e) => {
                    self.tx_rows.clear();
                    self.tx_status = Some(format!("Failed to decode transactions: {e}"));
                }
            },
            Err(e) => {
                self.tx_rows.clear();
                self.tx_cursor = 0;
                self.tx_status = Some(format!("Failed to load transactions: {e}"));
            }
        }
    }

    async fn submit_tx(&mut self) {
        let amount: u64 = self.tx_amount.parse().unwrap_or(0);
        let tx = TxIn {
            from: self.tx_from.clone(),
            to: self.tx_to.clone(),
            amount,
        };
        let base = &self.args.node;

        match self.http.post(format!("{base}/tx")).json(&tx).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                self.tx_status = Some(format!("POST /tx -> {status} {body}"));
            }
            Err(e) => self.tx_status = Some(format!("POST /tx failed: {e}")),
        }
    }

    async fn mine(&mut self) {
        let base = &self.args.node;
        let url = format!(
            "{base}/mine?target={}&data={}",
            self.mine_target,
            urlencoding::encode(&self.mine_data)
        );
        match self
            .http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.json::<MineResult>().await {
                Ok(mr) => {
                    if mr.mined {
                        self.mine_status = Some(format!(
                            "✅ Mined index={} nonce={} hash={} (target={})",
                            mr.index.unwrap_or_default(),
                            mr.nonce.unwrap_or_default(),
                            mr.hash.unwrap_or_default(),
                            mr.target.unwrap_or_default()
                        ));
                        self.refresh_dashboard().await;
                    } else {
                        self.mine_status =
                            Some(format!("❌ Mining reported failure: {:?}", mr.error));
                    }
                }
                Err(e) => self.mine_status = Some(format!("Decode /mine JSON failed: {e}")),
            },
            Err(e) => self.mine_status = Some(format!("GET /mine failed: {e}")),
        }
    }

    fn update_hash_demo(&mut self) {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(self.hash_input.as_bytes());
        self.hash_output = hex::encode(digest);
        self.hash_leading_zeros = count_leading_zero_bits(self.hash_output.as_bytes());
    }
}

fn count_leading_zero_bits(hex_bytes: &[u8]) -> u32 {
    // Count leading zero bits by scanning hex nybbles
    let mut bits = 0u32;
    for &b in hex_bytes {
        let v = match b {
            b'0' => 0,
            b'1'..=b'9' => b - b'0',
            b'a'..=b'f' => 10u8 + (b - b'a'),
            b'A'..=b'F' => 10u8 + (b - b'A'),
            _ => 0,
        };
        if v == 0 {
            bits += 4;
        } else {
            bits += v.leading_zeros() - 4;
            break;
        }
    }
    bits
}

#[tokio::main]
async fn main() -> Result<()> {
    // tracing
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let args = Args::parse();
    // terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.clone());
    app.refresh_dashboard().await;
    app.load_chain_page(None, 999, true).await;
    app.load_mempool_page().await;
    app.update_hash_demo();

    let res = run_app(&mut terminal, &mut app).await;

    // restore
    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::<B>(f, app))?;

        if crossterm::event::poll(Duration::from_millis(200))? {
            if let CEvent::Key(key) = event::read()? {
                if handle_key(app, key).await? {
                    break;
                }
            }
        }

        // periodic refresh (dashboard)
        if app.last_refresh.elapsed() >= Duration::from_secs(2) {
            app.refresh_dashboard().await;
        }
    }
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => return Ok(true),
        KeyCode::Esc => return Ok(true),
        KeyCode::Tab => {
            app.tab = match app.tab {
                Tab::Dashboard => Tab::Chain,
                Tab::Chain => Tab::Mempool,
                Tab::Mempool => Tab::Mine,
                Tab::Mine => Tab::HashDemo,
                Tab::HashDemo => Tab::Dashboard,
            };
        }
        KeyCode::BackTab => {
            app.tab = match app.tab {
                Tab::Dashboard => Tab::HashDemo,
                Tab::Chain => Tab::Dashboard,
                Tab::Mempool => Tab::Chain,
                Tab::Mine => Tab::Mempool,
                Tab::HashDemo => Tab::Mine,
            };
        }
        KeyCode::Char('r') => {
            app.refresh_dashboard().await;
            app.load_chain_page(None, 999, true).await;
            app.load_mempool_page().await;
        }
        // Chain view navigation
        KeyCode::Down => {
            if app.tab == Tab::Chain || app.tab == Tab::Mempool {
                app.next_row().await;
            }
        }
        KeyCode::Up => {
            if app.tab == Tab::Chain || app.tab == Tab::Mempool {
                app.previous_row().await;
            }
        }
        _ => {
            if app.tab == Tab::Mempool {
                match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => app.tx_amount.push(c),
                    KeyCode::Backspace => {
                        app.tx_amount.pop();
                    }
                    KeyCode::Enter => {
                        app.submit_tx().await;
                    }
                    _ => {}
                }
            } else if app.tab == Tab::Mine {
                match key.code {
                    KeyCode::Left => {
                        if app.mine_target > 0 {
                            app.mine_target -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app.mine_target < 32 {
                            app.mine_target += 1;
                        }
                    }
                    KeyCode::Char(c) if !c.is_control() => app.mine_data.push(c),
                    KeyCode::Backspace => {
                        app.mine_data.pop();
                    }
                    KeyCode::Enter => {
                        app.mine().await;
                    }
                    _ => {}
                }
            } else if app.tab == Tab::HashDemo {
                match key.code {
                    KeyCode::Char(c) if !c.is_control() => {
                        app.hash_input.push(c);
                        app.update_hash_demo();
                    }
                    KeyCode::Backspace => {
                        app.hash_input.pop();
                        app.update_hash_demo();
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}

fn ui<B: Backend>(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    // Tabs
    let titles = ["Dashboard", "Chain", "Mempool", "Mine", "HashDemo"]
        .iter()
        .map(|t| Line::from(*t))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(app.tab as usize)
        .block(Block::default().borders(Borders::ALL).title("ledger-tui"))
        .highlight_style(Style::default().fg(Color::Yellow));
    f.render_widget(tabs, chunks[0]);

    // Main area
    match app.tab {
        Tab::Dashboard => render_dashboard(f, chunks[1], app),
        Tab::Chain => render_chain(f, chunks[1], app),
        Tab::Mempool => render_mempool(f, chunks[1], app),
        Tab::Mine => render_mine(f, chunks[1], app),
        Tab::HashDemo => render_hashdemo(f, chunks[1], app),
    }

    // Footer
    let help = Paragraph::new(
        "q/ESC quit • TAB prev/next tab • r refresh • Mine: ←/→ target, Enter mine • HashDemo: type to hash • Mempool: Enter to POST /tx")
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title("help"));
    f.render_widget(help, chunks[2]);
}

fn render_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let mut lines = Vec::new();
    if let Some(h) = &app.head {
        lines.push(Line::from(format!("Head height: {}", h.height)));
    }
    if let Some(t) = &app.tip {
        lines.push(Line::from(format!("Tip height: {}", t.height)));
        lines.push(Line::from(format!(
            "Tip hash  : {}",
            t.hash.clone().unwrap_or_else(|| "-".into())
        )));
    }
    let dash =
        Paragraph::new(lines).block(Block::default().title("Overview").borders(Borders::ALL));
    f.render_widget(dash, chunks[0]);

    let about = Paragraph::new(vec![
        Line::from("ledger-tui"),
        Line::from("• Talks to /chain/head, /chain/tip, /chain/blocks"),
        Line::from("• Submits /tx and /mine"),
        Line::from("• Live SHA-256 hash demo"),
    ])
    .block(Block::default().title("About").borders(Borders::ALL));
    f.render_widget(about, chunks[1]);
}

fn render_chain(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = app.chain_rows.iter().enumerate().map(|(i, b)| {
        Row::new(vec![
            Cell::from(b.index.to_string()),
            Cell::from(b.ts.to_string()),
            Cell::from(b.nonce.to_string()),
            Cell::from(b.hash.clone()),
            Cell::from(b.previous_hash.clone()),
            Cell::from(b.tx_count.to_string()),
            Cell::from(b.merkle_root.clone()),
            Cell::from(b.data_hash.clone()),
            Cell::from(b.data.clone()),
        ])
        .style(if i == app.chain_cursor {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        })
    });
    let table = Table::new(
        rows,
        vec![
            Constraint::Length(6),
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Length(66),
            Constraint::Length(66),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["idx", "ts", "nonce", "hash", "prev", "txs"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title("Chain blocks"));
    f.render_stateful_widget(table, area, &mut app.chain_state);
}

fn render_mempool(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(30),
            Constraint::Min(0),
        ])
        .split(area);

    // Simple form
    let form = Paragraph::new(vec![
        Line::from(format!("From   : {}", app.tx_from)),
        Line::from(format!("To     : {}", app.tx_to)),
        Line::from(format!("Amount : {}", app.tx_amount)),
        Line::from("Press <Enter> to POST /tx"),
    ])
    .block(
        Block::default()
            .title("New transaction")
            .borders(Borders::ALL),
    );
    f.render_widget(form, chunks[0]);

    let status = Paragraph::new(app.tx_status.clone().unwrap_or_default())
        .block(Block::default().title("Status").borders(Borders::ALL));
    f.render_widget(status, chunks[1]);

    // render mempool transactions
    let rows = app.tx_rows.iter().enumerate().map(|(i, tx)| {
        Row::new(vec![
            Cell::from(i.to_string()),
            Cell::from(tx.from.to_string()),
            Cell::from(tx.to.to_string()),
            Cell::from(tx.amount.to_string()),
            Cell::from(tx.timestamp.to_string()),
        ])
        .style(if i == app.tx_cursor {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        })
    });
    let table = Table::new(
        rows,
        vec![
            Constraint::Length(6),
            Constraint::Length(45),
            Constraint::Length(45),
            Constraint::Length(16),
            Constraint::Length(11),
        ],
    )
    .header(
        Row::new(vec!["idx", "from", "to", "amount", "ts"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            // .scroll((app.tx_cursor as u16).saturating_sub(29), 0)
            .title("Mempool transactions"),
    );
    f.render_stateful_widget(table, chunks[2], &mut app.tx_state);

    let hint = Paragraph::new(
        "Tip: This is a minimal form (edit amount digits, use Enter). Extend as needed.",
    )
    .block(Block::default().title("Notes").borders(Borders::ALL));
    f.render_widget(hint, chunks[3]);
}

fn render_mine(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .split(area);

    let top = Paragraph::new(format!(
        "Target zeros: {}   (←/→ to adjust)",
        app.mine_target
    ))
    .block(Block::default().borders(Borders::ALL).title("Target"));
    f.render_widget(top, chunks[0]);

    let data = Paragraph::new(app.mine_data.clone()).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Block data (type, Backspace, Enter to mine)"),
    );
    f.render_widget(data, chunks[1]);

    let status = Paragraph::new(app.mine_status.clone().unwrap_or_default())
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[2]);
}

fn render_hashdemo(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(area);

    let input = Paragraph::new(app.hash_input.clone())
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[0]);

    let out = Paragraph::new(format!(
        "sha256: {}\nleading zero bits: {}",
        app.hash_output, app.hash_leading_zeros
    ))
    .block(Block::default().borders(Borders::ALL).title("Output"));
    f.render_widget(out, chunks[1]);

    let help = Paragraph::new(
        "Type to update the hash. Use this to visualise difficulty vs. leading-zeros.",
    )
    .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[2]);
}
