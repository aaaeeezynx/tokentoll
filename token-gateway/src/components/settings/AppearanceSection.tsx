/**
 * 設定頁的「外觀」區塊（P4.3 主題、P4.4 開機自啟、P4.7 視窗行為）。
 *
 * 從 `Settings.tsx` 拆出來：那支檔案原本已經 487 行，超過專案的 400 行上限。
 * 這一塊自己管自己的狀態（強調色、主題、自啟、關閉行為、輕量模式），
 * 不從外層接任何 props —— 拆出來之後責任也更清楚。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { ACCENTS, applyAccentVars, isAccentId, loadAccent, saveAccentLocal } from "../../lib/theme";
import {
  applyTheme,
  isThemePref,
  loadThemePref,
  resolveTheme,
  saveThemePref,
  watchSystemTheme,
  type ResolvedTheme,
  type ThemePref,
} from "../../lib/appearance";
import { Icon } from "../icons";
import { LANG_LABEL, LANGS, isLang, useI18n, type Lang } from "../../lib/i18n";
import { Toggle } from "../Toggle";
import { PopSelect } from "../PopSelect";
import { SectionHead } from "./SectionHead";

export function AppearanceSection() {
  const qc = useQueryClient();
  const { t, lang, setLang } = useI18n();
  const [accent, setAccent] = useState<string>(loadAccent());
  const [themePref, setThemePref] = useState<ThemePref>(loadThemePref());
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolveTheme(loadThemePref()));
  const [autoMsg, setAutoMsg] = useState("");
  const [winMsg, setWinMsg] = useState("");

  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settingsGet });
  const appearance = useQuery({
    queryKey: ["appearance_state"],
    queryFn: api.appearanceState,
  });
  const winBehavior = useQuery({
    queryKey: ["window_behavior"],
    queryFn: api.windowBehavior,
  });

  // 強調色（後端 SQLite 是真相源；localStorage 只做首幀快取）
  const pickAccent = (id: string) => {
    setAccent(id);
    applyAccentVars(id);
    saveAccentLocal(id);
    void api.settingsSetAccent(id).catch((e) => console.warn("accent persist failed:", e));
  };
  useEffect(() => {
    const a = settings.data?.accent;
    if (a && isAccentId(a) && a !== accent) {
      setAccent(a);
      applyAccentVars(a);
      saveAccentLocal(a);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.data]);

  // 主題：DB 是真相，載入後覆蓋快取
  const setThemeMut = useMutation({
    mutationFn: (t: ThemePref) => api.appearanceSetTheme(t),
    onError: (e) => setWinMsg(String(e)),
  });
  const pickTheme = (t: ThemePref) => {
    setThemePref(t);
    saveThemePref(t);
    setResolved(applyTheme(t));
    setThemeMut.mutate(t);
  };
  useEffect(() => {
    const t = appearance.data?.theme;
    if (isThemePref(t) && t !== themePref) {
      setThemePref(t);
      saveThemePref(t);
      setResolved(applyTheme(t));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appearance.data?.theme]);
  useEffect(
    () =>
      watchSystemTheme(() => {
        if (themePref === "system") setResolved(applyTheme("system"));
      }),
    [themePref],
  );

  // 開機自啟（P4.4）
  const autoMut = useMutation({
    mutationFn: (v: boolean) => api.appearanceSetAutostart(v),
    onSuccess: (s) => {
      setAutoMsg(t(s.autostart ? "settings.appearance.autostart.on" : "settings.appearance.autostart.off"));
      void qc.invalidateQueries({ queryKey: ["appearance_state"] });
    },
    onError: (e) => setAutoMsg(String(e)),
  });

  // 視窗行為（P4.7）
  const setWinMut = useMutation({
    mutationFn: (v: string) => api.windowSetCloseAction(v),
    onSuccess: (s) =>
      setWinMsg(
        t(
          s.close_action === "exit"
            ? "settings.appearance.close.exit.msg"
            : "settings.appearance.close.tray.msg",
        ),
      ),
    onError: (e) => setWinMsg(String(e)),
  });
  const winLight = useMutation({
    mutationFn: api.windowEnterLightweight,
    onError: (e) => setWinMsg(String(e)),
  });

  // 語言（P4.11）：切換即時生效（cc-switch 也是不用重啟）
  const setLangMut = useMutation({
    mutationFn: (l: string) => api.appearanceSetLang(l),
    onError: (e) => setWinMsg(String(e)),
  });
  const pickLang = (l: Lang) => {
    setLang(l);
    setLangMut.mutate(l);
  };

  // 後端的語言是唯一真相（切到其他視窗／重開都一致）
  useEffect(() => {
    const l = appearance.data?.lang;
    if (isLang(l) && l !== lang) setLang(l);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appearance.data?.lang]);

  return (
    <div className="glass p-5">
      <SectionHead
        icon="sliders"
        tile="linear-gradient(160deg, #b48cff, #7c5cff)"
        title={t("settings.appearance.title")}
        caption={t("settings.appearance.caption")}
      />

      {/* 語言（P4.11） */}
      <div className="mac-frow">
        <span className="mac-cap">{t("settings.lang.title")}</span>
        <div className="flex items-center gap-1.5">
          {LANGS.map((option) => (
            <button
              key={option}
              data-lang-option={option}
              className={`rounded-lg px-3 py-1.5 text-[12px] transition-colors ${
                lang === option
                  ? "bg-fg/[0.14] text-fg"
                  : "bg-fg/[0.05] text-fg/55 hover:bg-fg/[0.1]"
              }`}
              onClick={() => pickLang(option)}
            >
              {LANG_LABEL[option]}
            </button>
          ))}
          <span className="pl-1 text-[11px] text-fg/30">
            {t("settings.lang.caption")}
          </span>
        </div>
      </div>

      <div className="mac-frow">
        <span className="mac-cap">{t("settings.appearance.accent")}</span>
        <div className="flex items-center gap-2.5">
          {ACCENTS.map((a) => (
            <button
              key={a.id}
              title={a.label}
              onClick={() => pickAccent(a.id)}
              data-active={accent === a.id}
              className="accent-dot flex h-7 w-7 items-center justify-center rounded-full transition-transform hover:scale-110"
              style={{ background: a.value }}
            >
              {accent === a.id && <Icon name="check" size={13} className="text-fg" />}
            </button>
          ))}
          <span className="text-xs text-fg/40">
            {ACCENTS.find((a) => a.id === accent)?.label}
          </span>
        </div>
      </div>

      {/* 主題（P4.3）：跟隨系統／淺色／深色 */}
      <div className="mac-frow">
        <span className="mac-cap">{t("settings.appearance.theme")}</span>
        <div className="flex items-center gap-1.5">
          {(["system", "light", "dark"] as const).map((option) => (
            <button
              key={option}
              data-theme-option={option}
              className={`rounded-lg px-3 py-1.5 text-[12px] transition-colors ${
                themePref === option
                  ? "bg-fg/[0.14] text-fg"
                  : "bg-fg/[0.05] text-fg/55 hover:bg-fg/[0.1]"
              }`}
              onClick={() => pickTheme(option)}
            >
              {t(`settings.appearance.theme.${option}`)}
            </button>
          ))}
          <span className="pl-1 text-[11px] text-fg/30" data-theme-now={resolved}>
            {t("settings.appearance.theme.now", { actual: t(resolved === "light" ? "settings.appearance.theme.light" : "settings.appearance.theme.dark") })}
          </span>
        </div>
      </div>

      {/* 開機自啟（P4.4）：Windows 用登錄檔 Run 機碼（免管理員） */}
      <div className="mac-frow">
        <span className="mac-cap">{t("settings.appearance.autostart")}</span>
        <div className="flex items-center gap-2">
          <Toggle
            checked={appearance.data?.autostart ?? false}
            onChange={(v) => autoMut.mutate(v)}
            size="sm"
            disabled={appearance.isPending || autoMut.isPending}
          />
          <span className="text-xs text-fg/40">
            {t("settings.appearance.autostart.hint")}
          </span>
        </div>
      </div>
      {appearance.data?.autostart_command && (
        <p className="font-mono text-[10px] break-all text-fg/25">
          {appearance.data.autostart_command}
        </p>
      )}
      {autoMsg && <p className="pt-1 text-[11px] text-emerald-400/80">{autoMsg}</p>}

      {/* 關閉視窗時的行為（P4.7）：cc-switch 預設縮到系統匣 */}
      <div className="mac-frow">
        <span className="mac-cap">{t("settings.appearance.close")}</span>
        <div className="flex items-center gap-2">
          <div className="w-56">
            <PopSelect
              value={winBehavior.data?.close_action ?? "tray"}
              onChange={(v) => setWinMut.mutate(v)}
              options={[
                { value: "tray", label: t("settings.appearance.close.tray") },
                { value: "exit", label: t("settings.appearance.close.exit") },
              ]}
            />
          </div>
          <span className="text-[11px] text-fg/35">
            {t("settings.appearance.close.hint")}
          </span>
        </div>
      </div>

      {/* 輕量模式（P4.7）：銷毀視窗、只留托盤 */}
      <div className="mac-frow">
        <span className="mac-cap">{t("settings.appearance.lightweight")}</span>
        <div className="flex items-center gap-2">
          <button
            className="btn-ghost px-3 py-1.5 text-xs"
            onClick={() => {
              setWinMsg(t("settings.appearance.lightweight.msg"));
              winLight.mutate();
            }}
          >
            {t("settings.appearance.lightweight.btn")}
          </button>
          <span className="text-[11px] text-fg/35">
            {t("settings.appearance.lightweight.hint")}
          </span>
        </div>
      </div>
      {winMsg && <p className="pt-1 text-[11px] text-emerald-400/80">{winMsg}</p>}
    </div>
  );
}
