//! Render board states to SVG for blog graphics.
//!
//! Usage:
//!   # Single position from move sequence
//!   cargo run --release --bin render -- --moves "S(0,0) S(2,2) M(1,2)" -o board.svg
//!
//!   # Batch render all positions in a game
//!   cargo run --release --bin render -- --game "S(0,0) S(2,2) ..." --output-dir ./frames/
//!
//!   # From position encoding
//!   cargo run --release --bin render -- --position 18014398509481985 -o board.svg

use std::env;
use std::fs;

use gobblet_core::{Board, Move, Player, Pos, Size};

// ============================================================================
// Constants matching web UI CSS
// ============================================================================

const CELL_SIZE: f32 = 128.0;
const CELL_GAP: f32 = 8.0;
const BOARD_PADDING: f32 = 16.0;
const CELL_RADIUS: f32 = 8.0;
const BOARD_RADIUS: f32 = 12.0;

// Image padding (space around the entire content)
const IMAGE_PADDING: f32 = 16.0;

// Colors
const BG_COLOR: &str = "#1a1a1a";
const BOARD_BG: &str = "#2a2a2a";
const CELL_BG: &str = "#3a3a3a";
const P1_GRADIENT_START: &str = "#e74c3c";
const P1_GRADIENT_END: &str = "#c0392b";
const P2_GRADIENT_START: &str = "#3498db";
const P2_GRADIENT_END: &str = "#2980b9";

// Piece sizes (diameter)
const PIECE_SMALL: f32 = 38.0;
const PIECE_MEDIUM: f32 = 70.0;
const PIECE_LARGE: f32 = 100.0;

// Highlight colors
const HIGHLIGHT_LAST_MOVE: &str = "#f39c12";
const HIGHLIGHT_WINNER: &str = "#88cc88";

// Reserve panel (matching web UI)
const RESERVE_WIDTH: f32 = 208.0;  // (432 board - 16 gap) / 2
const RESERVE_HEIGHT: f32 = 100.0;  // Increased for more spacing
const RESERVE_GAP: f32 = 16.0;
const RESERVE_PADDING: f32 = 10.0;
const RESERVE_RADIUS: f32 = 10.0;
const RESERVE_PIECE_SMALL: f32 = 20.0;
const RESERVE_PIECE_MEDIUM: f32 = 34.0;
const RESERVE_PIECE_LARGE: f32 = 48.0;
const RESERVE_INACTIVE_OPACITY: f32 = 0.6;

// ============================================================================
// Move notation parser
// ============================================================================

/// Parse a single move from notation like "S(0,0)", "M(1,2)", "L(2,1)", "(0,0)->(1,1)"
fn parse_move(s: &str) -> Option<Move> {
    let s = s.trim();

    // Placement: S(r,c), M(r,c), L(r,c)
    if let Some(rest) = s.strip_prefix('S') {
        let (row, col) = parse_coords(rest)?;
        return Some(Move::Place { size: Size::Small, to: Pos::from_row_col(row, col) });
    }
    if let Some(rest) = s.strip_prefix('M') {
        let (row, col) = parse_coords(rest)?;
        return Some(Move::Place { size: Size::Medium, to: Pos::from_row_col(row, col) });
    }
    if let Some(rest) = s.strip_prefix('L') {
        let (row, col) = parse_coords(rest)?;
        return Some(Move::Place { size: Size::Large, to: Pos::from_row_col(row, col) });
    }

    // Slide: (r,c)->(r,c)
    if s.starts_with('(') && s.contains("->") {
        let parts: Vec<&str> = s.split("->").collect();
        if parts.len() == 2 {
            let (from_row, from_col) = parse_coords(parts[0])?;
            let (to_row, to_col) = parse_coords(parts[1])?;
            return Some(Move::Slide {
                from: Pos::from_row_col(from_row, from_col),
                to: Pos::from_row_col(to_row, to_col),
            });
        }
    }

    None
}

/// Parse "(r,c)" into (row, col)
fn parse_coords(s: &str) -> Option<(u8, u8)> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let row: u8 = parts[0].trim().parse().ok()?;
        let col: u8 = parts[1].trim().parse().ok()?;
        if row < 3 && col < 3 {
            return Some((row, col));
        }
    }
    None
}

/// Parse a sequence of moves separated by spaces
fn parse_moves(s: &str) -> Vec<Move> {
    s.split_whitespace()
        .filter_map(parse_move)
        .collect()
}

/// Apply a sequence of moves to a board, returning the final state
fn apply_moves(moves: &[Move]) -> Board {
    let mut board = Board::new();
    for &m in moves {
        board.apply(m);
    }
    board
}

// ============================================================================
// SVG generation
// ============================================================================

/// Options for rendering
#[derive(Default)]
struct RenderOptions {
    show_reserves: bool,
    highlight_cell: Option<Pos>,      // Last move destination
    highlight_winner: bool,           // Highlight winning line
    scale: f32,                       // Scale factor (1.0 = 128px cells)
}

/// Generate SVG for a board state
fn render_board_svg(board: &Board, opts: &RenderOptions) -> String {
    let scale = if opts.scale > 0.0 { opts.scale } else { 1.0 };

    // Calculate dimensions
    let board_inner = 3.0 * CELL_SIZE + 2.0 * CELL_GAP;
    let board_outer = board_inner + 2.0 * BOARD_PADDING;

    // Content dimensions (before image padding)
    let content_width = board_outer;
    let content_height = if opts.show_reserves {
        board_outer + RESERVE_HEIGHT + RESERVE_GAP
    } else {
        board_outer
    };

    // Total image dimensions (with padding on all sides)
    let width = content_width + 2.0 * IMAGE_PADDING;
    let height = content_height + 2.0 * IMAGE_PADDING;

    let scaled_width = width * scale;
    let scaled_height = height * scale;

    let mut svg = String::new();

    // SVG header with viewBox for scaling
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        scaled_width, scaled_height, width, height
    ));
    svg.push('\n');

    // Definitions (gradients)
    svg.push_str("  <defs>\n");
    svg.push_str(&format!(
        r#"    <linearGradient id="p1-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:{}"/>
      <stop offset="100%" style="stop-color:{}"/>
    </linearGradient>
"#, P1_GRADIENT_START, P1_GRADIENT_END));
    svg.push_str(&format!(
        r#"    <linearGradient id="p2-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:{}"/>
      <stop offset="100%" style="stop-color:{}"/>
    </linearGradient>
"#, P2_GRADIENT_START, P2_GRADIENT_END));
    svg.push_str("  </defs>\n");

    // Background
    svg.push_str(&format!(
        r#"  <rect width="{}" height="{}" fill="{}"/>"#,
        width, height, BG_COLOR
    ));
    svg.push('\n');

    // Get winning line if needed
    let winning_line: Vec<Pos> = if opts.highlight_winner {
        if let Some(winner) = board.check_winner() {
            board.winning_line(winner).map(|arr| arr.to_vec()).unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Board background (offset by image padding)
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}"/>"#,
        IMAGE_PADDING, IMAGE_PADDING, board_outer, board_outer, BOARD_RADIUS, BOARD_BG
    ));
    svg.push('\n');

    // Cells and pieces
    for row in 0..3 {
        for col in 0..3 {
            let pos = Pos::from_row_col(row, col);
            let x = IMAGE_PADDING + BOARD_PADDING + col as f32 * (CELL_SIZE + CELL_GAP);
            let y = IMAGE_PADDING + BOARD_PADDING + row as f32 * (CELL_SIZE + CELL_GAP);

            // Determine cell styling
            let is_highlighted = opts.highlight_cell == Some(pos);
            let is_winner = winning_line.contains(&pos);

            let (fill, stroke, stroke_width) = if is_winner {
                ("#4a5a3a", HIGHLIGHT_WINNER, 3.0)
            } else if is_highlighted {
                ("#5a4a3a", HIGHLIGHT_LAST_MOVE, 3.0)
            } else {
                (CELL_BG, "transparent", 0.0)
            };

            // Cell background
            svg.push_str(&format!(
                r#"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
                x, y, CELL_SIZE, CELL_SIZE, CELL_RADIUS, fill, stroke, stroke_width
            ));
            svg.push('\n');

            // Draw pieces (all layers, from bottom to top)
            let cx = x + CELL_SIZE / 2.0;
            let cy = y + CELL_SIZE / 2.0;

            for size in [Size::Large, Size::Medium, Size::Small] {
                if let Some(player) = board.piece_owner(pos, size) {
                    let diameter = match size {
                        Size::Small => PIECE_SMALL,
                        Size::Medium => PIECE_MEDIUM,
                        Size::Large => PIECE_LARGE,
                    };
                    let grad = match player {
                        Player::One => "url(#p1-grad)",
                        Player::Two => "url(#p2-grad)",
                    };

                    svg.push_str(&format!(
                        r#"  <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="rgba(0,0,0,0.2)" stroke-width="2"/>"#,
                        cx, cy, diameter / 2.0, grad
                    ));
                    svg.push('\n');
                }
            }
        }
    }

    // Reserves panel (if enabled)
    if opts.show_reserves {
        let reserves_x = IMAGE_PADDING;
        let reserves_y = IMAGE_PADDING + board_outer + RESERVE_GAP;
        svg.push_str(&render_reserves_svg(board, reserves_x, reserves_y, board_outer));
    }

    svg.push_str("</svg>\n");
    svg
}

/// Render reserves panel - two separate panels side by side
fn render_reserves_svg(board: &Board, x: f32, y: f32, total_width: f32) -> String {
    let mut svg = String::new();

    let current_player = board.current_player();
    let p1_reserves = board.reserves(Player::One);
    let p2_reserves = board.reserves(Player::Two);

    // Calculate panel width to fit within board width
    let panel_width = (total_width - RESERVE_GAP) / 2.0;

    // P1 reserves (left panel)
    let p1_active = current_player == Player::One;
    svg.push_str(&render_player_reserve_panel(
        Player::One,
        &p1_reserves,
        x,
        y,
        panel_width,
        p1_active,
    ));

    // P2 reserves (right panel)
    let p2_active = current_player == Player::Two;
    svg.push_str(&render_player_reserve_panel(
        Player::Two,
        &p2_reserves,
        x + panel_width + RESERVE_GAP,
        y,
        panel_width,
        p2_active,
    ));

    svg
}

/// Render a single player's reserve panel
fn render_player_reserve_panel(
    player: Player,
    reserves: &[u8; 3],
    x: f32,
    y: f32,
    width: f32,
    is_active: bool,
) -> String {
    let mut svg = String::new();

    let color = match player {
        Player::One => P1_GRADIENT_START,
        Player::Two => P2_GRADIENT_START,
    };
    let grad = match player {
        Player::One => "url(#p1-grad)",
        Player::Two => "url(#p2-grad)",
    };
    let label = match player {
        Player::One => "Player 1",
        Player::Two => "Player 2",
    };

    // Panel background with opacity for inactive player
    let opacity = if is_active { 1.0 } else { RESERVE_INACTIVE_OPACITY };
    svg.push_str(&format!(
        "  <g opacity=\"{}\">\n",
        opacity
    ));

    // Panel background
    svg.push_str(&format!(
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>\n",
        x, y, width, RESERVE_HEIGHT, RESERVE_RADIUS, BOARD_BG
    ));

    // Header label
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" font-family=\"system-ui, sans-serif\" font-size=\"13\" font-weight=\"bold\" fill=\"{}\" text-anchor=\"middle\">{}</text>\n",
        x + width / 2.0, y + 20.0, color, label
    ));

    // Piece slots - always show piece, count shows actual number
    let sizes = [
        (Size::Small, RESERVE_PIECE_SMALL, reserves[0]),
        (Size::Medium, RESERVE_PIECE_MEDIUM, reserves[1]),
        (Size::Large, RESERVE_PIECE_LARGE, reserves[2]),
    ];

    let slot_width = 60.0;
    let slots_total_width = 3.0 * slot_width;
    let start_x = x + (width - slots_total_width) / 2.0;

    for (i, (_, diameter, count)) in sizes.iter().enumerate() {
        let cx = start_x + i as f32 * slot_width + slot_width / 2.0;
        let cy = y + 48.0;  // Piece center position

        // Always draw piece circle (even when count is 0, just dimmed)
        let piece_opacity = if *count > 0 { 1.0 } else { 0.3 };
        svg.push_str(&format!(
            "    <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"rgba(0,0,0,0.2)\" stroke-width=\"2\" opacity=\"{}\"/>\n",
            cx, cy, diameter / 2.0, grad, piece_opacity
        ));

        // Count label - positioned below the largest piece with consistent spacing
        let count_y = y + RESERVE_HEIGHT - 8.0;
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" font-family=\"system-ui, sans-serif\" font-size=\"12\" fill=\"#888\" text-anchor=\"middle\">x{}</text>\n",
            cx, count_y, count
        ));
    }

    svg.push_str("  </g>\n");
    svg
}

// ============================================================================
// CLI
// ============================================================================

/// Names for the 8 D₄ symmetry transformations
const TRANSFORM_NAMES: [&str; 8] = [
    "identity",
    "rotate-90",
    "rotate-180",
    "rotate-270",
    "flip-horizontal",
    "flip-vertical",
    "flip-diagonal",
    "flip-antidiagonal",
];

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  render --moves \"S(0,0) S(2,2) ...\" -o board.svg");
    eprintln!("  render --game \"S(0,0) S(2,2) ...\" --output-dir ./frames/");
    eprintln!("  render --position <u64> -o board.svg");
    eprintln!("  render --moves \"...\" --symmetries --output-dir ./sym/");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --moves <notation>     Render board after applying moves");
    eprintln!("  --game <notation>      Render all positions in a game sequence");
    eprintln!("  --position <u64>       Render board from position encoding");
    eprintln!("  -o, --output <file>    Output file (default: board.svg)");
    eprintln!("  --output-dir <dir>     Output directory for batch mode");
    eprintln!("  --reserves             Include reserves panel");
    eprintln!("  --highlight-last       Highlight last move destination");
    eprintln!("  --highlight-winner     Highlight winning line");
    eprintln!("  --scale <float>        Scale factor (default: 1.0)");
    eprintln!("  --symmetries           Render all 8 D4 symmetry transformations");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let mut moves_str: Option<String> = None;
    let mut game_str: Option<String> = None;
    let mut position: Option<u64> = None;
    let mut output: Option<String> = None;
    let mut output_dir: Option<String> = None;
    let mut symmetries = false;
    let mut opts = RenderOptions::default();
    opts.scale = 1.0;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--moves" => {
                i += 1;
                moves_str = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--game" => {
                i += 1;
                game_str = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--position" => {
                i += 1;
                position = args.get(i).and_then(|s| s.parse().ok());
            }
            "-o" | "--output" => {
                i += 1;
                output = args.get(i).cloned();
            }
            "--output-dir" => {
                i += 1;
                output_dir = args.get(i).cloned();
            }
            "--reserves" => {
                opts.show_reserves = true;
            }
            "--highlight-last" => {
                // Will be set per-frame in batch mode
            }
            "--highlight-winner" => {
                opts.highlight_winner = true;
            }
            "--scale" => {
                i += 1;
                opts.scale = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            }
            "--symmetries" => {
                symmetries = true;
            }
            "-h" | "--help" => {
                print_usage();
                return;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Batch mode: render all positions in a game
    if let Some(game) = game_str {
        let dir = output_dir.unwrap_or_else(|| ".".to_string());
        fs::create_dir_all(&dir).expect("Failed to create output directory");

        let moves = parse_moves(&game);
        let mut board = Board::new();

        // Initial position
        let svg = render_board_svg(&board, &opts);
        let path = format!("{}/move-00.svg", dir);
        fs::write(&path, &svg).expect("Failed to write SVG");
        println!("Wrote {}", path);

        // After each move
        for (i, &m) in moves.iter().enumerate() {
            board.apply(m);

            // Highlight last move destination
            let mut frame_opts = RenderOptions {
                show_reserves: opts.show_reserves,
                highlight_winner: opts.highlight_winner,
                scale: opts.scale,
                highlight_cell: match m {
                    Move::Place { to, .. } => Some(to),
                    Move::Slide { to, .. } => Some(to),
                },
            };

            // Check for winner on this move
            if board.check_winner().is_some() {
                frame_opts.highlight_winner = true;
            }

            let svg = render_board_svg(&board, &frame_opts);
            let path = format!("{}/move-{:02}.svg", dir, i + 1);
            fs::write(&path, &svg).expect("Failed to write SVG");
            println!("Wrote {}", path);
        }

        println!("\nRendered {} frames", moves.len() + 1);
        return;
    }

    // Symmetries mode: render all 8 D4 transformations
    if symmetries {
        let dir = output_dir.unwrap_or_else(|| ".".to_string());
        fs::create_dir_all(&dir).expect("Failed to create output directory");

        let board = if let Some(ref moves) = moves_str {
            apply_moves(&parse_moves(moves))
        } else if let Some(pos) = position {
            Board::from_u64(pos)
        } else {
            eprintln!("Error: --symmetries requires --moves or --position");
            print_usage();
            std::process::exit(1);
        };

        // Render all 8 transformations
        for t in 0..8 {
            let transformed = Board::from_u64(board.transform(t));
            let svg = render_board_svg(&transformed, &opts);
            let path = format!("{}/sym-{}-{}.svg", dir, t, TRANSFORM_NAMES[t]);
            fs::write(&path, &svg).expect("Failed to write SVG");
            println!("Wrote {} ({})", path, TRANSFORM_NAMES[t]);
        }

        println!("\nRendered 8 symmetry transformations");
        return;
    }

    // Single position mode
    let board = if let Some(moves) = moves_str {
        apply_moves(&parse_moves(&moves))
    } else if let Some(pos) = position {
        Board::from_u64(pos)
    } else {
        eprintln!("Error: specify --moves, --game, or --position");
        print_usage();
        std::process::exit(1);
    };

    let svg = render_board_svg(&board, &opts);

    let out_path = output.unwrap_or_else(|| "board.svg".to_string());
    fs::write(&out_path, &svg).expect("Failed to write SVG");
    println!("Wrote {}", out_path);
}
