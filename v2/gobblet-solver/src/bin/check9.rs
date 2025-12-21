use gobblet_core::{Board, Player, Size, Pos};

fn main() {
    // Create position: P1 Small gobbled by P2 Medium at (0,0)
    let mut board = Board::new();
    board.push_piece(Pos::from_row_col(0, 0), Player::One, Size::Small);
    board.push_piece(Pos::from_row_col(0, 0), Player::Two, Size::Medium);
    
    println!("Created position: P1 Small + P2 Medium at (0,0)");
    println!("  raw u64: {}", board.to_u64());
    println!("  canonical: {}", board.canonical());
    
    // Check encoding of raw 9
    println!("\nDecoding raw 9:");
    let board9 = Board::from_u64(9);
    println!("  Board::from_u64(9).to_u64() = {}", board9.to_u64());
    println!("  Board::from_u64(9).canonical() = {}", board9.canonical());
    
    // Check what cell (0,0) contains
    if let Some((player, size)) = board9.top_piece(Pos::from_row_col(0, 0)) {
        println!("  top piece at (0,0): {:?} {:?}", player, size);
    } else {
        println!("  no piece at (0,0)");
    }
    
    // Check all 8 symmetries of the board we created
    println!("\nAll symmetries of our created board:");
    for t in 0..8 {
        let sym = board.transform(t);
        println!("  transform {}: {}", t, sym);
    }
    println!("  min (canonical): {}", board.canonical());
}
