import { useLocation } from "react-router";

/**
 * 官報パーサ playground（`/kanpo-playground`）を出すか。開発サーバーでは常に出す。
 * 本番ではクエリ `?kanpo-parser=1` で有効にし（`#/kanpo-playground?kanpo-parser=1` のように
 * ハッシュ側のクエリでもよい）、以後はこのブラウザで有効のまま。`?kanpo-parser=0` で無効に戻す。
 */
const PARAM = "kanpo-parser";
/** 一時的に本番でも常に出す。隠す運用に戻すときは false にする（URL・localStorage の切り替えが効くようになる）。 */
const ALWAYS_ENABLED = true;
const STORAGE_KEY = "kanpo-parser-enabled";

/** URL（`?…` とハッシュ側の `#/…?…` の両方）に指定があれば true/false、無ければ null。 */
function fromUrl(): boolean | null {
  const hashQuery = window.location.hash.includes("?") ? window.location.hash.slice(window.location.hash.indexOf("?")) : "";
  for (const query of [window.location.search, hashQuery]) {
    const v = new URLSearchParams(query).get(PARAM);
    if (v === null) continue;
    return !["0", "off", "false"].includes(v.toLowerCase());
  }
  return null;
}

export function isKanpoParserEnabled(): boolean {
  if (import.meta.env.DEV || ALWAYS_ENABLED) return true;
  const requested = fromUrl();
  try {
    if (requested !== null) {
      localStorage.setItem(STORAGE_KEY, requested ? "on" : "off");
      return requested;
    }
    return localStorage.getItem(STORAGE_KEY) === "on";
  } catch {
    // 保存できない環境では URL の指定だけで決める。
    return requested ?? false;
  }
}

/** 画面遷移のたびに URL を見直す（遷移先のクエリで有効・無効を切り替えられるように）。 */
export function useKanpoParserEnabled(): boolean {
  useLocation();
  return isKanpoParserEnabled();
}
