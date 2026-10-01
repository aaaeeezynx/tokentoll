/**
 * 多語系（P4.11）。
 *
 * cc-switch 的語言切換是**即時生效、不用重啟**（手冊 §1.5），我們照做：
 * 偏好存 `settings.lang`（後端是唯一真相），localStorage 只做首幀快取。
 *
 * ## 為什麼只有繁中／英文兩種
 *
 * cc-switch 有四種（簡中／繁中／英文／日文）。我們先做繁中與英文：
 * 簡中與繁中差異集中在用詞（軟體／軟件、資訊／信息…），
 * 日文更需要有母語的人校對 —— 沒有把握的翻譯比沒有翻譯更糟，
 * 所以在對齊矩陣上標明「只做兩種」。
 *
 * ## 未翻譯的字串怎麼處理
 *
 * `t()` 找不到 key 就**退回繁中原文**（字典的 source 語言），
 * 所以開英文時會看到「英文外殼 ＋ 繁中內文」—— 這是覆蓋率推進中的正常狀態，
 * 實際覆蓋率用 `.workbuddy/tmp/i18n_cover.py` 量（見 docs/TESTING.md §0.9.32）。
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type Lang = "zh-TW" | "en";

const KEY = "tg:lang:v1";

export const LANG_LABEL: Record<Lang, string> = {
  "zh-TW": "繁體中文",
  en: "English",
};

/** 可選語言（給語言選擇器走訪）。 */
export const LANGS: Lang[] = ["zh-TW", "en"];

export function isLang(v: unknown): v is Lang {
  return typeof v === "string" && (LANGS as string[]).includes(v);
}

export function loadLang(): Lang {
  try {
    const v = localStorage.getItem(KEY);
    if (isLang(v)) return v;
  } catch {
    /* 忽略 */
  }
  return "zh-TW";
}

export function saveLang(l: Lang) {
  try {
    localStorage.setItem(KEY, l);
  } catch {
    /* 忽略 */
  }
}

/** 字典：key → 繁中（source）／英文。 */
import { DICT } from "./i18nDict";

export type Dict = Record<string, [string, string]>;

type Ctx = {
  lang: Lang;
  /** 取字串；`{name}` 形式的佔位會被替換。 */
  t: (key: string, vars?: Record<string, string | number>) => string;
  setLang: (l: Lang) => void;
};

const LangCtx = createContext<Ctx>({
  lang: "zh-TW",
  t: (k) => DICT[k]?.[0] ?? k,
  setLang: () => {},
});

/** 在字典之外取字串。 */
function translate(lang: Lang, key: string): string {
  const row = DICT[key];
  if (!row) return key;
  return lang === "en" ? row[1] : row[0];
}

export function I18nProvider(props: {
  initial: Lang;
  /** 由 App 呼叫：把後端回傳的語言（唯一真相）同步進來。 */
  onRemote?: (l: Lang) => void;
  children: ReactNode;
}) {
  const [lang, setLangState] = useState<Lang>(props.initial);

  const setLang = useCallback(
    (l: Lang) => {
      setLangState(l);
      saveLang(l);
      document.documentElement.lang = l;
      props.onRemote?.(l);
    },
    [props],
  );

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => {
      let s = translate(lang, key);
      if (vars) {
        for (const [k, v] of Object.entries(vars)) {
          // target 是 ES2020，沒有 replaceAll → 用 split/join
          s = s.split(`{${k}}`).join(String(v));
        }
      }
      return s;
    },
    [lang],
  );

  const value = useMemo(() => ({ lang, t, setLang }), [lang, t, setLang]);
  return <LangCtx.Provider value={value}>{props.children}</LangCtx.Provider>;
}

export function useI18n(): Ctx {
  return useContext(LangCtx);
}

/** 字典 key 數（給設定頁與文件報覆蓋率用）。 */
export const DICT_SIZE = Object.keys(DICT).length;
