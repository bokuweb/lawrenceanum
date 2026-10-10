import { useEffect, useState } from "react";

/**
 * グラフの「模様でも区別する」設定。色の区別がつきにくい人・印刷・強制カラーモードのための補助で、
 * 既定ではオフ (細かい斜線は常時だと読みにくく、見る人によっては負担になるため)。
 * 設定画面のスイッチか、OS の強制カラーモード (`forced-colors: active`) でオンになる。
 */
const STORAGE_KEY = "chart-patterns";
const EVENT = "chart-patterns-change";

function readSetting(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "on";
  } catch {
    return false;
  }
}

export function setChartPatterns(on: boolean) {
  try {
    localStorage.setItem(STORAGE_KEY, on ? "on" : "off");
  } catch {
    // 保存できない環境 (プライベートモード等) でも、このページ内では切り替える。
  }
  window.dispatchEvent(new CustomEvent(EVENT, { detail: on }));
}

/** [有効か (設定 or 強制カラー), 設定値] */
export function useChartPatterns(): [boolean, boolean] {
  const [setting, setSetting] = useState(readSetting);
  const [forced, setForced] = useState(() => window.matchMedia?.("(forced-colors: active)").matches ?? false);
  useEffect(() => {
    const onChange = (e: Event) => setSetting((e as CustomEvent<boolean>).detail);
    window.addEventListener(EVENT, onChange);
    const mq = window.matchMedia?.("(forced-colors: active)");
    const onMq = () => setForced(mq.matches);
    mq?.addEventListener("change", onMq);
    return () => {
      window.removeEventListener(EVENT, onChange);
      mq?.removeEventListener("change", onMq);
    };
  }, []);
  return [setting || forced, setting];
}

/** 系列ごとの模様。0 は無地、以降は 45° / 135° の線と、その交差。 */
export type PatternKind = "solid" | "lines45" | "lines135" | "cross";
export const PATTERN_ORDER: PatternKind[] = ["solid", "lines45", "lines135", "cross"];

/** SVG の <defs> に置く模様。地色は系列の色、線は同系の暗い色 (tone-on-tone)。
 *  パターン全体を 45° / 135° 回転させ、タイルの縁に沿った線を引くと継ぎ目の無い斜線になる。 */
export function PatternDef({ id, color, kind }: { id: string; color: string; kind: PatternKind }) {
  const ink = "rgba(0,0,0,0.45)";
  const angle = kind === "lines135" ? 135 : 45;
  return (
    <pattern id={id} patternUnits="userSpaceOnUse" width={5} height={5} patternTransform={`rotate(${angle})`}>
      <rect width={5} height={5} fill={color} />
      {kind !== "solid" && <line x1={0} y1={0} x2={0} y2={5} stroke={ink} strokeWidth={2} />}
      {kind === "cross" && <line x1={0} y1={0} x2={5} y2={0} stroke={ink} strokeWidth={2} />}
    </pattern>
  );
}

/** 凡例・ツールチップ用の小さな見本。模様が有効なら同じ模様で塗る。 */
export function Swatch({ color, kind, patterns, id }: { color: string; kind: PatternKind; patterns: boolean; id: string }) {
  return (
    <svg className="size-2.5 shrink-0 rounded-sm" viewBox="0 0 10 10" aria-hidden>
      {patterns && (
        <defs>
          <PatternDef id={id} color={color} kind={kind} />
        </defs>
      )}
      <rect width={10} height={10} fill={patterns ? `url(#${id})` : color} />
    </svg>
  );
}
