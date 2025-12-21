// API client for the Gobblet Gobblers backend

import type { GameState, History, LegalMove } from "./types";

const API_BASE = "http://localhost:8000";

export async function getGame(): Promise<GameState> {
  const res = await fetch(`${API_BASE}/game`);
  if (!res.ok) throw new Error("Failed to fetch game state");
  return res.json();
}

export async function getMoves(): Promise<LegalMove[]> {
  const res = await fetch(`${API_BASE}/moves`);
  if (!res.ok) throw new Error("Failed to fetch moves");
  return res.json();
}

export async function makeMove(move: {
  to_row: number;
  to_col: number;
  from_row?: number;
  from_col?: number;
  size?: number;
}): Promise<GameState> {
  const res = await fetch(`${API_BASE}/move`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(move),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to make move");
  }
  return res.json();
}

export async function resetGame(): Promise<GameState> {
  const res = await fetch(`${API_BASE}/reset`, { method: "POST" });
  if (!res.ok) throw new Error("Failed to reset game");
  return res.json();
}

export async function getHistory(): Promise<History> {
  const res = await fetch(`${API_BASE}/history`);
  if (!res.ok) throw new Error("Failed to fetch history");
  return res.json();
}

export async function undo(): Promise<GameState> {
  const res = await fetch(`${API_BASE}/undo`, { method: "POST" });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to undo");
  }
  return res.json();
}

export async function redo(): Promise<GameState> {
  const res = await fetch(`${API_BASE}/redo`, { method: "POST" });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to redo");
  }
  return res.json();
}

export async function gotoMove(moveIndex: number): Promise<GameState> {
  const res = await fetch(`${API_BASE}/goto/${moveIndex}`, { method: "POST" });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to go to move");
  }
  return res.json();
}

export async function exportGame(): Promise<string> {
  const res = await fetch(`${API_BASE}/export`);
  if (!res.ok) throw new Error("Failed to export game");
  const data = await res.json();
  return data.notation;
}

export async function importGame(notation: string): Promise<GameState> {
  const res = await fetch(`${API_BASE}/import`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ notation }),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to import game");
  }
  return res.json();
}

export async function exportState(): Promise<string> {
  const res = await fetch(`${API_BASE}/state/export`);
  if (!res.ok) throw new Error("Failed to export state");
  const data = await res.json();
  return String(data.encoding);
}

export async function importState(encoding: string): Promise<GameState> {
  const res = await fetch(`${API_BASE}/state/import`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ encoding: parseInt(encoding, 10) }),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.detail || "Failed to import state");
  }
  return res.json();
}
