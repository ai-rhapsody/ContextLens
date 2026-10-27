import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { chatPending, convoGet, dataUrl, PROVIDERS, TASKS, type Conversation } from "../lib/api";
import { KIND_LABEL, kindOf } from "../lib/files";
import { Answer, FollowBar, SelectionMenu, Sources, Thread, useStickToBottom, useThread } from "../lib/convo";

/** The conversation window: one capture, its answer and every follow-up, with room to keep asking. */
export function Chat() {
  const [convo, setConvo] = useState<Conversation | null>(null);
  const [missing, setMissing] = useState(false);
  const [openSrc, setOpenSrc] = useState(false);
  const current = useRef<number | null>(null);
  const thread = useThread(convo?.id ?? null);
  const body = useRef<HTMLDivElement>(null);
  useStickToBottom(body, thread.items);

  const load = useCallback(async (id: number | null) => {
    if (id === null) return;
    const c = await convoGet(id);
    current.current = id;
    setMissing(!c);
    setConvo(c);
    setOpenSrc(false);
    thread.reset(c?.thread ?? []);
    requestAnimationFrame(() => body.current && (body.current.scrollTop = body.current.scrollHeight));
  }, [thread.reset]);

  useEffect(() => {
    chatPending().then(load);
    const un = listen<number>("chat-open", ({ payload }) => load(payload));
    return () => { un.then((f) => f()); };
  }, [load]);

  // Another window (the popup) added a follow-up to this conversation.
  const seen = useRef({ busy: false, count: 0 });
  seen.current = { busy: thread.busy, count: thread.items.length };
  useEffect(() => {
    const un = listen<number>("convo-updated", async ({ payload }) => {
      if (payload !== current.current || seen.current.busy) return;
      const c = await convoGet(payload);
      if (c && c.thread.length > seen.current.count) {
        setConvo(c);
        thread.reset(c.thread);
      }
    });
    return () => { un.then((f) => f()); };
  }, [thread.reset]);

  if (missing) return <div className="chat"><p className="boot">找不到這段對話，可能已從歷史紀錄刪除。</p></div>;
  if (!convo) return <div className="chat"><p className="boot">載入中…</p></div>;

  const kind = convo.media ? kindOf(convo.media.mime) : null;
  const provider = PROVIDERS.find((p) => p.id === convo.provider)?.name;
  const taskLabel = TASKS.find((t) => t.id === convo.task)?.label;

  return (
    <div className="chat">
      <div className="chat-head">
        <span className="chat-task">{taskLabel}{convo.tutor ? " · 教學" : ""}</span>
        <span className="chat-title">{convo.text || convo.label || "對話"}</span>
      </div>

      <div className="chat-body" ref={body}>
        <div className="chat-col">
          {convo.media && (
            <div className="src-media">
              {kind === "image" && <img src={dataUrl(convo.media)} alt={convo.media.name ?? "擷取的圖片"} />}
              {kind === "audio" && <audio controls src={dataUrl(convo.media)} />}
              {kind === "video" && <video controls src={dataUrl(convo.media)} />}
            </div>
          )}
          {!convo.media && convo.text && !/^##\s*原文/m.test(convo.answer) && (
            <p className={`src${openSrc ? " open" : ""}`} onClick={() => setOpenSrc((v) => !v)} title="點一下展開原文">
              {convo.text}
            </p>
          )}
          {convo.media_lost && (
            <p className="faint chat-note">
              原本的{convo.label ? `「${convo.label}」` : KIND_LABEL.image}沒有保存，追問會根據上面的回答。
            </p>
          )}
          <div className="ex first"><Answer text={convo.answer} task={convo.task} streaming={false} /></div>
          <Sources list={convo.sources.map(([title, url]) => ({ title, url }))} />
          <Thread items={thread.items} />
        </div>
      </div>

      <div className="chat-foot">
        <div className="chat-col">
          <FollowBar task={convo.task} busy={thread.busy} onAsk={thread.ask} autoFocus />
          <p className="faint chat-meta">{provider} · {convo.model} · 在回答裡選取文字可以直接追問</p>
        </div>
      </div>
      <SelectionMenu root={body} busy={thread.busy} onAsk={thread.ask} />
    </div>
  );
}
