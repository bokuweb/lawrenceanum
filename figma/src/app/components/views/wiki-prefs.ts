import { useEffect, useState } from "react";

/**
 * 要約前 (LLM の文章が無く、機械が作った一覧だけ) の wiki ページを表示するか。
 * 既定は表示しない。wiki トップの切り替えで変え、一覧・グラフ・件数・法令詳細の導線に共通で効く。
 */
const STORAGE_KEY = "wiki-show-stubs";
const EVENT = "wiki-show-stubs-change";

function read(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "on";
  } catch {
    return false;
  }
}

export function setShowStubs(on: boolean) {
  try {
    localStorage.setItem(STORAGE_KEY, on ? "on" : "off");
  } catch {
    // 保存できない環境でも、このページ内では切り替える。
  }
  window.dispatchEvent(new CustomEvent(EVENT, { detail: on }));
}

export function useShowStubs(): boolean {
  const [on, setOn] = useState(read);
  useEffect(() => {
    const onChange = (e: Event) => setOn((e as CustomEvent<boolean>).detail);
    window.addEventListener(EVENT, onChange);
    return () => window.removeEventListener(EVENT, onChange);
  }, []);
  return on;
}
