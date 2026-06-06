import { useEffect } from "react";
import "./AboutModal.css";

interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function AboutModal({ isOpen, onClose }: AboutModalProps) {
  // Close on Escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    if (isOpen) {
      document.addEventListener("keydown", handleEscape);
      document.body.style.overflow = "hidden";
    }
    return () => {
      document.removeEventListener("keydown", handleEscape);
      document.body.style.overflow = "unset";
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="about-overlay" onClick={onClose}>
      <div className="about-modal" onClick={(e) => e.stopPropagation()}>
        <div className="about-header">
          <h2>How This Works</h2>
          <button className="about-close" onClick={onClose}>
            ×
          </button>
        </div>

        <div className="about-content">
          <section>
            <h3>What is Gobblet Gobblers?</h3>
            <p>
              Gobblet Gobblers is a two-player strategy game, like Tic-Tac-Toe
              with a twist. It's the 3×3 version of{" "}
              <a
                href="https://en.wikipedia.org/wiki/Gobblet"
                target="_blank"
                rel="noopener noreferrer"
              >
                Gobblet
              </a>
              .
            </p>
            <p>
              Each player has 6 pieces in 3 sizes (2 small, 2 medium, 2 large).
              On your turn, you can either place a piece from your reserve onto
              the board, or move one of your pieces already on the board. The
              key mechanic: larger pieces can "gobble" (cover) smaller ones.
              Get 3 of your pieces visible in a row to win!
            </p>
          </section>

          <section>
            <h3>What is a Tablebase?</h3>
            <p>
              A tablebase is a database of every possible game position, each
              pre-calculated to show whether it's a{" "}
              <span className="win">win</span>,{" "}
              <span className="loss">loss</span>, or{" "}
              <span className="draw">draw</span> with perfect play.
            </p>
            <p>
              This tablebase covers every position reachable from the start,{" "}
              <strong>over 531 million</strong> of them, solved exactly with
              retrograde analysis. Each position also stores its distance to the
              result.
            </p>
          </section>

          <section>
            <h3>The Game is Solved: Player 1 Wins</h3>
            <p>
              With perfect play, <strong>Player 1 (Orange) always wins</strong>.
              The colored move indicators show optimal play:
            </p>
            <ul>
              <li>
                <span className="win">Green</span> = winning move. The number is
                how many plies until you win (W13 = win in 13).
              </li>
              <li>
                <span className="draw">Yellow</span> = drawing move
              </li>
              <li>
                <span className="loss">Red</span> = losing move. The number is
                how many plies until you lose.
              </li>
              <li>
                <span className="unknown">Gray (?)</span> = couldn't reach the
                tablebase
              </li>
            </ul>
          </section>

          <section>
            <h3>Explore the Solution</h3>
            <p>Try this to see the solution in action:</p>
            <ol>
              <li>
                Play as Player 1 and always pick a{" "}
                <span className="win">green</span> move
              </li>
              <li>
                Notice that every move available to Player 2 is{" "}
                <span className="loss">red</span> (losing)
              </li>
              <li>No matter what Player 2 does, Player 1 forces a win!</li>
            </ol>
            <p>
              If Player 1 makes a mistake and plays a{" "}
              <span className="draw">yellow</span> move, the game can end in a
              draw through threefold repetition (same position occurring three
              times).
            </p>
          </section>

          <section>
            <h3>How It Was Built</h3>
            <p>
              The game was solved with <strong>retrograde analysis</strong>:
              enumerate every reachable position, then work backward from
              finished games to label each one a win, loss, or draw plus its
              distance to the result. Draws are the positions where neither side
              can force a win and best play only repeats.
            </p>
            <p>
              The game logic runs in your browser via WebAssembly (compiled from
              Rust), so moves are instant. Only the tablebase lookups go to the
              server.
            </p>
          </section>
        </div>

        <div className="about-footer">
          <button className="about-close-button" onClick={onClose}>
            Got it!
          </button>
        </div>
      </div>
    </div>
  );
}
