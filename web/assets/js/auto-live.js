/* Transición de navegación, no sondeo: reutiliza el estado del cliente.
   Una navegación por partida; los cortes de conexión no vuelven a imponerla. */
const BETWEEN_GAMES = new Set([
  "Lobby", "Matchmaking", "ReadyCheck", "ChampSelect",
  "WaitingForStats", "PreEndOfGame", "EndOfGame",
]);

export function createAutoLiveTransition() {
  let entered = false;
  let gameId = null;
  return (status, enabled = true, currentGameId = null) => {
    const phase = status?.phase;
    const playing = Boolean(
      status?.inGame || status?.liveClient ||
      ["GameStart", "InProgress", "Reconnect"].includes(phase)
    );
    if (!playing) {
      if (BETWEEN_GAMES.has(phase)) {
        entered = false;
        gameId = null;
      }
      return false;
    }
    const nextId = currentGameId ? String(currentGameId) : null;
    if (nextId && gameId && nextId !== gameId) entered = false;
    if (nextId) gameId = nextId;
    if (!enabled || entered) return false;
    entered = true;
    return true;
  };
}
