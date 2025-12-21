# Deployment Planning

## Current State

- **Frontend:** Vite + React + TypeScript
- **Backend:** FastAPI (Python)
- **Database:** SQLite with 19.9M solved positions (294 MB)

## Deployment Options Discussed

### Frontend Hosting

**Vercel** (Recommended)
- Free tier available
- Excellent for static React apps
- Automatic deploys from GitHub

### Backend Hosting

| Option | Pros | Cons |
|--------|------|------|
| **Railway** | Easy Python deployment, free tier | Limited free tier resources |
| **Render** | Good free tier, auto-deploys | Cold starts on free tier |
| **Fly.io** | Global edge deployment | More complex setup |

### Database Options

1. **Ship SQLite with backend** - Simplest, 294MB file size is manageable
2. **PlanetScale/Turso** - Serverless SQL if needed
3. **Make tablebase public** - Downloadable SQLite for researchers/enthusiasts

## Considerations

### Tablebase Distribution

The solved positions database could be valuable to:
- Board game enthusiasts
- AI/ML researchers
- Educational purposes

Options:
- Host downloadable SQLite file
- Provide API access
- Both

### Resource Requirements

- Backend memory: Moderate (only serving queries, not solving)
- Storage: 294 MB for tablebase
- Compute: Low for serving, high for solving new positions

## Priority

**Full solve completion takes priority over deployment.**

Deployment can proceed with the current partial tablebase (99.7% of positions), but completing the full solve would provide:
- Complete game tree coverage
- All P1 moves available (not just optimal ones)
- More comprehensive analysis capability

---

*Status: Planning stage. Focus currently on completing full solve.*
