import "./Header.css";

interface HeaderProps {
  onHowItWorksClick: () => void;
}

export function Header({ onHowItWorksClick }: HeaderProps) {
  return (
    <header className="site-header">
      <div className="header-content">
        <div className="header-brand">
          <span className="header-title">Gobblet Gobblers, solved</span>
          <span className="header-tagline">
            A 3×3 children's game, played to the last move.
          </span>
        </div>

        <nav className="header-nav">
          <button onClick={onHowItWorksClick} className="header-navbtn">
            How it works
          </button>
          <a
            href="https://brianhliou.com/posts/gobblet-gobblers/"
            target="_blank"
            rel="noopener noreferrer"
            className="header-link"
          >
            Write-up
          </a>
          <a
            href="https://github.com/brianhliou/gobblet-gobblers"
            target="_blank"
            rel="noopener noreferrer"
            className="header-link"
          >
            GitHub
          </a>
          <a
            href="https://brianhliou.com/"
            target="_blank"
            rel="noopener noreferrer"
            className="header-link"
          >
            Brian Liou
          </a>
        </nav>
      </div>
    </header>
  );
}
