import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type UsageRange } from "../lib/api";
import { ACCENTS, applyAccentVars, isAccentId, loadAccent, saveAccentLocal } from "../lib/theme";
import {
  THEME_LABEL,
  applyTheme,
  isThemePref,
  loadThemePref,
  resolveTheme,
  saveThemePref,
  watchSystemTheme,
  type ResolvedTheme,
  type ThemePref,
} from "../lib/appearance";
import { Icon } from "./icons";
import { RangePicker, type CustomRange } from "./RangePicker";
import { Toggle } from "./Toggle";
import { PopSelect } from "./PopSelect";

function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

function SectionHead(props: {
  icon: "switch" | "sliders" | "download" | "info";
  tile: string;
  title: string;
  caption?: string;
}) {
  return (
    <div className="mb-3 flex items-center gap-2.5">
      <span
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-fg"
        style={{ background: props.tile }}
      >
        <Icon name={props.icon} size={17} />
      </span>
      <div className="min-w-0">
        <div className="text-[15px] font-semibold tracking-tight text-fg">
          {props.title}
        </div>
        {props.caption && (
          <div className="truncate text-[11px] text-fg/30">
            {props.caption}
          </div>
        )}
      </div>
    </div>
  );
}

export default function SettingsPage() {
  const qc = useQueryClient();
  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
  });
  const dir = useQuery({ queryKey: ["app_data_dir"], queryFn: api.appDataDir });

  const [port, setPort] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const [msg, setMsg] = useState("");
  const [accent, setAccent] = useState<string>(loadAccent());
  const [csvRange, setCsvRange] = useState<UsageRange>("30d");
  const [csvCustom, setCsvCustom] = useState<CustomRange | null>(null);
  const [csvMsg, setCsvMsg] = useState("");
  const [exporting, setExporting] = useState(false);

  // 主題與開機自啟（P4.3／P4.4）
  const [themePref, setThemePref] = useState<ThemePref>(loadThemePref());
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolveTheme(loadThemePref()));
  const [autoMsg, setAutoMsg] = useState("");
  const appearance = useQuery({
    queryKey: ["appearance_state"],
    queryFn: api.appearanceState,
  });
  const autoMut = useMutation({
    mutationFn: (v: boolean) => api.appearanceSetAutostart(v),
    onSuccess: (s) => {
      setAutoMsg(s.autostart ? "已開啟開機自啟" : "已關閉開機自啟");
      void qc.invalidateQueries({ queryKey: ["appearance_state"] });
    },
    onError: (e) => setAutoMsg(String(e)),
  });
  const setAutoStartEnabled = (v: boolean) => autoMut.mutate(v);

  // 視窗行為（P4.7）
  const [winMsg, setWinMsg] = useState("");
  const winBehavior = useQuery({
    queryKey: ["window_behavior"],
    queryFn: api.windowBehavior,
  });
  const setWinMut = useMutation({
    mutationFn: (v: string) => api.windowSetCloseAction(v),
    onSuccess: (s) =>
      setWinMsg(
        s.close_action === "exit"
          ? "關閉視窗時會直接結束（網關也會停）"
          : "關閉視窗時縮到系統匣，網關繼續運作",
      ),
    onError: (e) => setWinMsg(String(e)),
  });
  const winLight = useMutation({
    mutationFn: api.windowEnterLightweight,
    onError: (e) => setWinMsg(String(e)),
  });

  // DB 是唯一真相：載入後用資料庫的值覆蓋快取
  useEffect(() => {
    const t = appearance.data?.theme;
    if (isThemePref(t) && t !== themePref) {
      setThemePref(t);
      saveThemePref(t);
      setResolved(applyTheme(t));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appearance.data?.theme]);

  // 跟隨系統：系統切換時即時反映
  useEffect(() => {
    return watchSystemTheme(() => {
      if (themePref === "system") setResolved(applyTheme("system"));
    });
  }, [themePref]);

  const pickTheme = (t: ThemePref) => {
    setThemePref(t);
    saveThemePref(t);
    setResolved(applyTheme(t));
    setThemeMut.mutate(t);
  };
  const setThemeMut = useMutation({
    mutationFn: (t: ThemePref) => api.appearanceSetTheme(t),
    onError: (e) => setProtoMsg(String(e)),
  });

  // Deep Link 協定（P4.1）
  const proto = useQuery({
    queryKey: ["deeplink_protocol"],
    queryFn: api.deeplinkProtocolState,
  });
  const [protoMsg, setProtoMsg] = useState("");
  const reg = useMutation({
    mutationFn: api.deeplinkRegister,
    onSuccess: (s) => {
      setProtoMsg(s.registered ? "已註冊" : "註冊後仍未生效，請檢查權限");
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setProtoMsg(String(e)),
  });
  const unreg = useMutation({
    mutationFn: api.deeplinkUnregister,
    onSuccess: () => {
      setProtoMsg("已取消註冊（之後點連結不會再打開這個 App）");
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setProtoMsg(String(e)),
  });

  useEffect(() => {
    if (settings.data) {
      setPort(String(settings.data.gateway_port));
      setAutoStart(settings.data.auto_start_proxy);
    }
  }, [settings.data]);

  const check = useMutation({
    mutationFn: () => api.proxyCheckPort(Number(port)),
    onSuccess: () => setMsg(`連接埠 ${port} 可用`),
    onError: (e) => setMsg(String(e)),
  });

  const save = useMutation({
    mutationFn: () => api.settingsSet(Number(port), autoStart),
    onSuccess: (s) => {
      setMsg(`已儲存：連接埠 ${s.gateway_port}，開機自啟${s.auto_start_proxy ? "開" : "關"}`);
      void qc.invalidateQueries({ queryKey: ["settings"] });
      void qc.invalidateQueries({ queryKey: ["tools"] });
    },
    onError: (e) => setMsg(String(e)),
  });

  const pickAccent = (id: string) => {
    setAccent(id);
    applyAccentVars(id);
    saveAccentLocal(id);
    // 後端 SQLite 持久化（真相源）；失敗僅告警，下次啟動以後端值為準
    void api.settingsSetAccent(id).catch((e) => console.warn("accent persist failed:", e));
  };

  // 後端真相源同步（修復 localStorage 刷盤丟失導致的回退）
  useEffect(() => {
    const a = settings.data?.accent;
    if (a && isAccentId(a) && a !== accent) {
      setAccent(a);
      applyAccentVars(a);
      saveAccentLocal(a);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.data]);

  const exportCsv = async () => {
    setExporting(true);
    setCsvMsg("");
    try {
      const csv = await api.usageExportCsv(
        csvRange,
        null,
        null,
        null,
        csvCustom?.start ?? null,
        csvCustom?.end ?? null,
      );
      const blob = new Blob(["\uFEFF" + csv], {
        type: "text/csv;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const d = new Date();
      const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
      const rangeTag = csvCustom
        ? `custom-${csvCustom.start}-${csvCustom.end}`
        : csvRange;
      const a = document.createElement("a");
      a.href = url;
      a.download = `token-gateway-usage-${rangeTag}-${stamp}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      const rows = csv.trim().split("\n").length - 1;
      setCsvMsg(`已匯出 ${rows} 筆`);
    } catch (e) {
      setCsvMsg(`匯出失敗：${String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* ── 網關 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="switch"
          tile="linear-gradient(160deg, #3395ff, #0a64d8)"
          title="網關"
          caption="本機代理監聽（僅 127.0.0.1）"
        />
        {settings.isPending ? (
          <p className="text-sm text-fg/30">載入中…</p>
        ) : (
          <div>
            <div className="mac-frow">
              <span className="mac-cap">監聽連接埠</span>
              <div className="flex items-center gap-2">
                <input
                  className="field w-full px-3 py-1.5 text-[13px]"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                />
                <button
                  className="btn-ghost shrink-0 px-3.5 py-1.5 text-[13px]"
                  onClick={() => check.mutate()}
                >
                  檢測
                </button>
              </div>
            </div>
            <div className="mac-frow">
              <span className="mac-cap">自動啟動</span>
              <div className="flex items-center gap-2">
                <Toggle checked={autoStart} onChange={setAutoStart} size="sm" />
                <span className="text-xs text-fg/40">
                  應用程式啟動時自動啟動網關
                </span>
              </div>
            </div>
            {msg && <p className="pt-1.5 text-xs text-fg/50">{msg}</p>}
            <p className="pt-1.5 text-[11px] text-fg/25">
              15721 常被 cc-switch 佔用，預設 15722。網關執行時不可改連接埠（需先停止）；改完後已切換工具需重新執行一次切換。
            </p>
            <div className="flex justify-end pt-2.5">
              <button
                className="btn-primary px-5 py-2 text-sm disabled:opacity-40"
                disabled={save.isPending}
                onClick={() => save.mutate()}
              >
                儲存
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── 外觀 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="sliders"
          tile="linear-gradient(160deg, #b48cff, #7c5cff)"
          title="外觀"
          caption="強調色即時套用全站"
        />
        <div className="mac-frow">
          <span className="mac-cap">強調色</span>
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
                {accent === a.id && (
                  <Icon name="check" size={13} className="text-fg" />
                )}
              </button>
            ))}
            <span className="text-xs text-fg/40">
              {ACCENTS.find((a) => a.id === accent)?.label}
            </span>
          </div>
        </div>

        {/* 主題（P4.3）：跟隨系統／淺色／深色 */}
        <div className="mac-frow">
          <span className="mac-cap">主題</span>
          <div className="flex items-center gap-1.5">
            {(["system", "light", "dark"] as const).map((t) => (
              <button
                key={t}
                data-theme-option={t}
                className={`rounded-lg px-3 py-1.5 text-[12px] transition-colors ${
                  themePref === t
                    ? "bg-fg/[0.14] text-fg"
                    : "bg-fg/[0.05] text-fg/55 hover:bg-fg/[0.1]"
                }`}
                onClick={() => pickTheme(t)}
              >
                {THEME_LABEL[t]}
              </button>
            ))}
            <span className="pl-1 text-[11px] text-fg/30" data-theme-now={resolved}>
              目前：{resolved === "light" ? "淺色" : "深色"}
            </span>
          </div>
        </div>

        {/* 開機自啟（P4.4）：Windows 用登錄檔 Run 機碼（免管理員） */}
        <div className="mac-frow">
          <span className="mac-cap">開機自啟</span>
          <div className="flex items-center gap-2">
            <Toggle
              checked={appearance.data?.autostart ?? false}
              onChange={(v) => setAutoStartEnabled(v)}
              size="sm"
              disabled={appearance.isPending || autoMut.isPending}
            />
            <span className="text-xs text-fg/40">
              登入 Windows 時自動啟動（寫 HKCU 的 Run 機碼）
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
          <span className="mac-cap">關閉視窗時</span>
          <div className="flex items-center gap-2">
            <div className="w-56">
              <PopSelect
                value={winBehavior.data?.close_action ?? "tray"}
                onChange={(v) => setWinMut.mutate(v)}
                options={[
                  { value: "tray", label: "縮到系統匣（建議）" },
                  { value: "exit", label: "直接結束" },
                ]}
              />
            </div>
            <span className="text-[11px] text-fg/35">
              這是本地網關 —— 縮到系統匣才不會斷掉工具的流量
            </span>
          </div>
        </div>

        {/* 輕量模式（P4.7）：銷毀視窗、只留托盤 */}
        <div className="mac-frow">
          <span className="mac-cap">輕量模式</span>
          <div className="flex items-center gap-2">
            <button
              className="btn-ghost px-3 py-1.5 text-xs"
              onClick={() => {
                setWinMsg("已進入輕量模式（可從托盤「開啟主視窗」或點 Deep Link 回來）");
                winLight.mutate();
              }}
            >
              進入輕量模式
            </button>
            <span className="text-[11px] text-fg/35">
              釋放視窗記憶體，托盤與網關照常運作
            </span>
          </div>
        </div>
        {winMsg && <p className="pt-1 text-[11px] text-emerald-400/80">{winMsg}</p>}
      </div>

      {/* ── 用量匯出 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="download"
          tile="linear-gradient(160deg, #34d399, #0a9e6e)"
          title="用量匯出"
          caption="請求明細 CSV（含 BOM，Excel 可直接開）"
        />
        <div className="mac-frow">
          <span className="mac-cap">範圍</span>
          <div className="flex flex-wrap items-center gap-2">
            <RangePicker
              presets={[
                { id: "today", label: "今日" },
                { id: "7d", label: "最近 7 天" },
                { id: "30d", label: "最近 30 天" },
              ]}
              range={csvRange}
              custom={csvCustom}
              onPreset={(v) => setCsvRange(v as UsageRange)}
              onCustom={setCsvCustom}
              btnClassName="px-2.5 py-1 text-xs font-medium"
            />
            <button
              className="btn-ghost flex shrink-0 items-center gap-1.5 px-3.5 py-1.5 text-[13px] disabled:opacity-40"
              disabled={exporting}
              onClick={() => void exportCsv()}
            >
              <Icon name="download" size={13} />
              {exporting ? "匯出中…" : "匯出 CSV"}
            </button>
          </div>
        </div>
        {csvMsg && <p className="pt-1.5 text-xs text-fg/50">{csvMsg}</p>}
      </div>

      {/* ── Deep Link 協定（P4.1） ── */}
      <div className="glass p-5">
        <SectionHead
          icon="download"
          tile="linear-gradient(160deg, #0A84FF, #0055cc)"
          title="Deep Link 匯入"
          caption="tokengateway:// 一鍵匯入來源／提示詞／MCP／技能"
        />
        <div className="mac-frow">
          <span className="mac-cap">協定註冊</span>
          <div className="min-w-0 flex-1">
            <p className="text-[12px] text-fg/70">
              {proto.isPending
                ? "查詢中…"
                : proto.data?.registered
                  ? "已註冊（HKCU\\Software\\Classes\\tokengateway）"
                  : "尚未註冊"}
            </p>
            {proto.data?.command && (
              <p className="font-mono text-[10px] break-all text-fg/25">
                {proto.data.command}
              </p>
            )}
          </div>
          <button
            className="btn-ghost shrink-0 px-3 py-1 text-xs disabled:opacity-40"
            disabled={reg.isPending}
            onClick={() => reg.mutate()}
          >
            重新註冊
          </button>
          {proto.data?.registered && (
            <button
              className="btn-ghost shrink-0 px-3 py-1 text-xs text-red-400/80 disabled:opacity-40"
              disabled={unreg.isPending}
              onClick={() => unreg.mutate()}
            >
              取消註冊
            </button>
          )}
        </div>
        <p className="text-[11px] leading-relaxed text-fg/30">
          連結格式（與 cc-switch 的 <span className="font-mono">ccswitch://</span> 相同參數）：
          <span className="font-mono"> tokengateway://v1/import?resource=provider&app=claude&name=…&endpoint=…</span>。
          匯入前一定會先跳出預覽確認；帶有 API Key 的連結只顯示遮罩後的值。
        </p>
        {protoMsg && <p className="pt-1 text-[11px] text-emerald-400/80">{protoMsg}</p>}
      </div>

      {/* ── 關於 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="info"
          tile="linear-gradient(160deg, #8e8e93, #48484e)"
          title="關於"
          caption="Token Gateway v2 · Tauri 本地網關"
        />
        <div className="mac-frow">
          <span className="mac-cap">數據目錄</span>
          <div className="flex items-center gap-2">
            <p className="min-w-0 flex-1 font-mono text-[11px] break-all text-fg/45">
              {dir.isPending ? "讀取中…" : dir.isError ? "讀取失敗" : dir.data}
            </p>
            {!dir.isPending && !dir.isError && dir.data && (
              <button
                className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1 text-xs"
                onClick={() => copyText(dir.data as string)}
              >
                <Icon name="copy" size={12} />
                複製
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
