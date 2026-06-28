import { useEffect, useState } from "react";

export function getOnlineStatus(): boolean {
  if (typeof navigator === "undefined" || typeof navigator.onLine !== "boolean") {
    return true;
  }
  return navigator.onLine;
}

export function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(getOnlineStatus);
  useEffect(() => {
    const update = () => setOnline(getOnlineStatus());
    window.addEventListener("online", update);
    window.addEventListener("offline", update);
    update();
    return () => {
      window.removeEventListener("online", update);
      window.removeEventListener("offline", update);
    };
  }, []);
  return online;
}
