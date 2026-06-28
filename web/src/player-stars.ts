import { useCallback, useEffect, useState } from "react";
import { getStarred, star, unstar, waveFeedback } from "./api";
import type { WaveSession } from "./auth";

export function useStarredTracks(session: WaveSession) {
  const [starredIds, setStarredIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    getStarred(session)
      .then((starred) => {
        if (!cancelled) setStarredIds(new Set(starred.songs.map((song) => song.id)));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [session]);

  const isStarred = useCallback((id: string) => starredIds.has(id), [starredIds]);

  const toggleStar = useCallback(
    (id: string) => {
      setStarredIds((prevSet) => {
        const nextSet = new Set(prevSet);
        const wasStarred = nextSet.has(id);
        if (wasStarred) nextSet.delete(id);
        else nextSet.add(id);
        if (!wasStarred) {
          waveFeedback(session, id, "like").catch(() => undefined);
        }
        (wasStarred ? unstar(session, id) : star(session, id)).catch(() => {
          setStarredIds((current) => {
            const reverted = new Set(current);
            if (wasStarred) reverted.add(id);
            else reverted.delete(id);
            return reverted;
          });
        });
        return nextSet;
      });
    },
    [session],
  );

  return { isStarred, toggleStar };
}
