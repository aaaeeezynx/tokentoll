/**
 * 設定頁的「Deep Link 匯入」區塊（P4.1 協定註冊狀態）。
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { SectionHead } from "./SectionHead";
import { useI18n } from "../../lib/i18n";

export function DeeplinkSection() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [msg, setMsg] = useState("");
  const proto = useQuery({
    queryKey: ["deeplink_protocol"],
    queryFn: api.deeplinkProtocolState,
  });
  const reg = useMutation({
    mutationFn: api.deeplinkRegister,
    onSuccess: () => {
      setMsg(t("settings.deeplink.done"));
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setMsg(String(e)),
  });
  const unreg = useMutation({
    mutationFn: api.deeplinkUnregister,
    onSuccess: () => {
      setMsg(t("settings.deeplink.undone"));
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setMsg(String(e)),
  });

  return (
    <div className="glass p-5">
      <SectionHead
        icon="download"
        tile="linear-gradient(160deg, #0A84FF, #0055cc)"
        title={t("settings.deeplink.title")}
        caption={t("settings.deeplink.caption")}
      />
      <div className="mac-frow">
        <span className="mac-cap">協定註冊</span>
        <div className="min-w-0 flex-1">
          <p className="text-[12px] text-fg/70">
            {proto.isPending
              ? t("common.loading")
              : proto.data?.registered
                ? t("settings.deeplink.registered")
                : t("settings.deeplink.missing")}
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
          {t("settings.deeplink.reregister")}
        </button>
        {proto.data?.registered && (
          <button
            className="btn-ghost shrink-0 px-3 py-1 text-xs text-red-400/80 disabled:opacity-40"
            disabled={unreg.isPending}
            onClick={() => unreg.mutate()}
          >
            {t("settings.deeplink.unregister")}
          </button>
        )}
      </div>
      <p className="text-[11px] leading-relaxed text-fg/30">
        連結格式（與 cc-switch 的 <span className="font-mono">ccswitch://</span> 相同參數）：
        <span className="font-mono">
          {" "}
          tokengateway://v1/import?resource=provider&app=claude&name=…&endpoint=…
        </span>
        。匯入前一定會先跳出預覽確認；帶有 API Key 的連結只顯示遮罩後的值。
      </p>
      {msg && <p className="pt-1 text-[11px] text-emerald-400/80">{msg}</p>}
    </div>
  );
}
