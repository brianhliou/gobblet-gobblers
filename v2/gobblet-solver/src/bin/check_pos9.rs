use gobblet_core::{Board, Player, Size, Pos, Move};

fn main() {
    // Simulate the game that leads to position 9
    let mut board = Board::new();
    
    println!("=== Simulating path to canonical 9 ===");
    println!("Initial: raw={}", board.to_u64());
    
    // P1 places Small at (0,0)
    let m1 = Move::Place { size: Size::Small, to: Pos::from_row_col(0, 0) };
    board.apply(m1);
    println!("After P1 S(0,0): raw={} canonical={}", board.to_u64(), board.canonical());
    
    // P2 places Medium at (0,0) - gobbles
    let m2 = Move::Place { size: Size::Medium, to: Pos::from_row_col(0, 0) };
    board.apply(m2);
    println!("After P2 M(0,0): raw={} canonical={}", board.to_u64(), board.canonical());
    
    // This should be canonical 9!
    println!("\nExpected canonical 9, got: {}", board.canonical());
    println!("Match: {}", board.canonical() == 9);
}
