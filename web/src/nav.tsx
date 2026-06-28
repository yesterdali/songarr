import { createContext, useContext } from "react";

export type Route =
  | { name: "home" }
  | { name: "search" }
  | { name: "library" }
  | { name: "albums" }
  | { name: "playlists" }
  | { name: "liked" }
  | { name: "imports" }
  | { name: "artist"; id: string; title: string }
  | { name: "artistLookup"; title: string }
  | { name: "album"; id: string; title: string }
  | { name: "playlist"; id: string; title: string }
  | { name: "settings" };

export type TabName = "home" | "search" | "library" | "playlists";

export type Nav = {
  route: Route;
  push: (route: Route) => void;
  back: () => void;
  setTab: (tab: TabName) => void;
  canGoBack: boolean;
};

const NavContext = createContext<Nav | null>(null);

export const NavProvider = NavContext.Provider;

export function useNav(): Nav {
  const nav = useContext(NavContext);
  if (!nav) throw new Error("useNav used outside NavProvider");
  return nav;
}
