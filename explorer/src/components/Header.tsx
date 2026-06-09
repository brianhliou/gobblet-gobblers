import "./Header.css";

interface HeaderProps {
  onHowItWorksClick: () => void;
}

export function Header({ onHowItWorksClick }: HeaderProps) {
  return (
    <header className="site-header">
      <h1>Gobblet Gobblers, solved</h1>
      <span className="tagline">
        A 3×3 children's game, played to the last move.
      </span>

      <nav className="hlinks">
        <button onClick={onHowItWorksClick} className="navbtn">
          How it works
        </button>
        <a
          href="https://brianhliou.com/posts/gobblet-gobblers/"
          target="_blank"
          rel="noopener noreferrer"
        >
          Write-up
        </a>
        <a
          href="https://github.com/brianhliou/gobblet-gobblers"
          target="_blank"
          rel="noopener noreferrer"
        >
          GitHub
        </a>
        <a
          href="https://brianhliou.com/"
          target="_blank"
          rel="noopener noreferrer"
        >
          Brian Liou
        </a>
      </nav>
    </header>
  );
}
