use gobblet_core::{Board, Player, Size, Pos};

fn main() {
    // Test 1: Symmetric positions should have same canonical
    println!("=== Test 1: Corner symmetry ===");
    let corners = [(0,0), (0,2), (2,0), (2,2)];
    let mut canonicals = Vec::new();
    
    for (r, c) in corners {
        let mut board = Board::new();
        board.push_piece(Pos::from_row_col(r, c), Player::One, Size::Small);
        let canon = board.canonical();
        println!("P1 Small at ({},{}): raw={:016x} canonical={:016x}", r, c, board.to_u64(), canon);
        canonicals.push(canon);
    }
    
    let all_same = canonicals.iter().all(|&c| c == canonicals[0]);
    println!("All corners same canonical: {}\n", all_same);
    
    // Test 2: Check if initial position canonical is 0
    println!("=== Test 2: Initial position ===");
    let board = Board::new();
    println!("Initial position raw: {:016x}", board.to_u64());
    println!("Initial position canonical: {:016x}\n", board.canonical());
    
    // Test 3: Compare one position in both V1 and V2 format
    println!("=== Test 3: Single piece position ===");
    let mut board = Board::new();
    board.push_piece(Pos::from_row_col(0, 0), Player::One, Size::Large);
    println!("P1 Large at (0,0), P1 to move:");
    println!("  raw={:016x} canonical={:016x}", board.to_u64(), board.canonical());
}
