import "./Header.css";

interface HeaderProps {
  onHowItWorksClick: () => void;
}

export function Header({ onHowItWorksClick }: HeaderProps) {
  return (
    <header className="site-header">
      <div className="header-content">
        <div className="header-spacer" />

        <h1 className="header-title">Gobblet Gobblers Tablebase</h1>

        <nav className="header-nav">
          <button onClick={onHowItWorksClick} className="header-button">
            ? How it Works
          </button>
          <a
            href="https://brianhliou.github.io/"
            target="_blank"
            rel="noopener noreferrer"
            className="header-link"
          >
            Brian Liou
          </a>
          <a
            href="https://github.com/brianhliou"
            target="_blank"
            rel="noopener noreferrer"
            className="header-link"
          >
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}
