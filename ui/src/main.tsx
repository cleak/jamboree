import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { JSX } from "solid-js";
import { render } from "solid-js/web";
import "./index.css";

type BusEvent = {
  id: string;
  subject: string;
  payload: string;
  receivedAt: Date;
  envelope: Record<string, unknown>;
  source: "recent" | "live";
};

type WireBusEvent = {
  subject?: unknown;
  payload?: unknown;
};

type NotifyPayload = {
  urgency?: string;
  summary?: string;
  payload?: unknown;
};

type HumanNotification = {
  id: string;
  subject: string;
  urgency: "low" | "medium" | "high" | "critical";
  summary: string;
  detail: string;
  receivedAt: string;
};

type PickerRow = {
  sessionId: string;
  taskId: string;
  harness: string;
  status: string;
  worktreePath: string;
  lastEvent: string;
  updatedAt: Date;
};

type SessionOutputRecord = {
  id: string;
  sessionId: string;
  taskId: string;
  traceId: string;
  stream: "stdout" | "stderr" | "output";
  line: string;
  ts: Date;
  sequence: number;
  truncated: boolean;
};

type ParsedLogLine = {
  kind: "assistant" | "user" | "tool" | "status" | "error" | "text";
  title: string;
  body: string;
  meta: string[];
};

type TraceRow = {
  traceId: string;
  events: number;
  latestSubject: string;
  updatedAt: Date;
};

type TraceReplay = {
  requested_trace_id: string;
  max_depth: number;
  chain: string[];
  entries: TraceReplayEntry[];
};

type TraceReplayEntry = {
  path: string;
  line_number: number;
  event_type: string;
  timestamp: string;
  journal_seq: number;
  trace_id: string;
  parent_trace_id?: string | null;
  actor: string;
  payload: unknown;
};

type TraceReplayState =
  | { status: "idle" }
  | { status: "loading"; traceId: string }
  | { status: "loaded"; traceId: string; replay: TraceReplay }
  | { status: "error"; traceId: string; error: string };

type MessageMode = "queue" | "interrupt" | "full-stop";
type TaskTarget = "blueberry" | "jamboree";
type ThemeMode = "light" | "dark" | "system";

type MessageResponse = {
  message_id: string;
  session_id: string;
  mode: MessageMode;
  status: string;
  subject: string;
  trace_id: string;
  detail: unknown;
};

type MessageOutboxItem = {
  messageId: string;
  sessionId: string;
  mode: MessageMode;
  status: string;
  text: string;
  detail: string;
  updatedAt: Date;
};

type QuotaRow = {
  key: string;
  status: string;
  remaining: string;
  usage: string;
  detail: string;
  source: string;
  updatedAt: Date;
};

type ServiceRow = {
  name: string;
  status: string;
  ready: string;
  pid: number;
  restarts: number;
  uptime: string;
  running: boolean;
};

type TaskRow = {
  taskId: string;
  description: string;
  project: string;
  taskClass: string;
  priority: string;
  status: string;
  requestedBy: string;
  prRef: string;
  sessionId: string;
  harness: string;
  outcome: string;
  traceId: string;
  updatedAt: Date;
};

type TaskGraphRow = {
  task_id?: string;
  taskId?: string;
  description?: string;
  project?: string;
  task_class?: string;
  taskClass?: string;
  priority?: string;
  status?: string;
  requested_by?: string;
  requestedBy?: string;
  pr_ref?: string;
  prRef?: string;
  session_id?: string;
  sessionId?: string;
  harness?: string;
  outcome?: string;
  trace_id?: string;
  traceId?: string;
  updated_at?: string;
  updatedAt?: string;
};

type PrRow = {
  prRef: string;
  taskId: string;
  title: string;
  status: string;
  ciStatus: string;
  review: string;
  updatedAt: Date;
  /// Latest `picker.continuation-needed.attempt` seen for the task linked
  /// to this PR. >= CONTINUATION_ATTEMPT_CAP (5) means the picker gave up.
  continuationAttempt: number;
  /// Latest `picker.continuation-needed.reason` for context in the UI.
  continuationReason: string;
};

type QuotaSnapshot = {
  fetched_at?: string;
  fetchedAt?: string;
  source?: string;
  windows?: unknown;
};

type Stat = {
  label: string;
  value: string;
  detail: string;
};

type CreateTaskState =
  | { status: "idle" }
  | { status: "submitting" }
  | { status: "created"; taskId: string; traceId: string }
  | { status: "error"; message: string };

type DeployTargetRow = {
  shortName: string;
  crateName: string;
  binaryName: string;
  strategy: string;
};

type DeployState =
  | { status: "idle" }
  | { status: "deploying"; service: string }
  | { status: "ok"; service: string; version: string; detail: string; traceId: string }
  | { status: "error"; service: string; message: string };

type TimelineItem = {
  key: string;
  status: string;
  title: string;
  detail: string;
  actor: string;
  updatedAt: Date;
};

type RouteData = {
  status: string;
  subject: string;
  lastConnectedAt: Date | null;
  events: BusEvent[];
  notifications: HumanNotification[];
  services: ServiceRow[];
  taskRows: TaskRow[];
  pickerRows: PickerRow[];
  prRows: PrRow[];
  traceRows: TraceRow[];
  quotaRows: QuotaRow[];
  quotaError: string;
  quotaRefreshedAt: Date | null;
  maestroEvents: BusEvent[];
  traceReplay: TraceReplayState;
  token: string;
  createTaskState: CreateTaskState;
  onCreateTaskState: (state: CreateTaskState) => void;
  deployTargets: DeployTargetRow[];
};

const navItems = [
  { href: "/", label: "Dashboard" },
  { href: "/tasks", label: "Tasks" },
  { href: "/prs", label: "PRs" },
  { href: "/pickers", label: "Pickers" },
  { href: "/maestro", label: "Maestro" },
  { href: "/journal", label: "Journal" },
  { href: "/traces", label: "Traces" },
  { href: "/quotas", label: "Quotas" },
  { href: "/health", label: "Health" },
  { href: "/settings", label: "Settings" }
];

function App() {
  const [token, setToken] = createSignal(localStorage.getItem("jam.ui.token") ?? "");
  const [theme, setTheme] = createSignal<ThemeMode>(loadTheme());
  const [connectedToken, setConnectedToken] = createSignal("");
  const [subject, setSubject] = createSignal("journal.>");
  const [status, setStatus] = createSignal("disconnected");
  const [events, setEvents] = createSignal<BusEvent[]>([]);
  const [notifications, setNotifications] = createSignal<HumanNotification[]>([]);
  const [services, setServices] = createSignal<ServiceRow[]>([]);
  const [taskSnapshots, setTaskSnapshots] = createSignal<TaskRow[]>([]);
  const [quotaSnapshot, setQuotaSnapshot] = createSignal<QuotaSnapshot | null>(null);
  const [quotaError, setQuotaError] = createSignal("");
  const [quotaRefreshedAt, setQuotaRefreshedAt] = createSignal<Date | null>(null);
  const [createTaskState, setCreateTaskState] = createSignal<CreateTaskState>({ status: "idle" });
  const [deployTargets, setDeployTargets] = createSignal<DeployTargetRow[]>([]);
  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const [lastConnectedAt, setLastConnectedAt] = createSignal<Date | null>(null);
  const [traceReplay, setTraceReplay] = createSignal<TraceReplayState>({ status: "idle" });
  const currentPath = createMemo(() => normalizePath(window.location.pathname));
  const traceDetailId = createMemo(() => traceIdFromPath(currentPath()));
  const unreadCount = createMemo(() => notifications().length);
  let streamSocket: WebSocket | undefined;
  let autoConnectedToken = "";

  createEffect(() => {
    const next = token().trim();
    if (next.length > 0) {
      localStorage.setItem("jam.ui.token", next);
    }
  });

  createEffect(() => {
    localStorage.setItem("jam.ui.theme", theme());
  });

  // Re-render when the OS color scheme changes, but only while we're in
  // `system` mode. `effective` is read in the JSX below, so updating a
  // signal whenever the media query flips is the simplest way to pick up
  // OS toggles without a full reload.
  const [systemDark, setSystemDark] = createSignal(
    typeof window !== "undefined" && window.matchMedia
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : false
  );
  createEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    mq.addEventListener("change", onChange);
    onCleanup(() => mq.removeEventListener("change", onChange));
  });
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const effective = createMemo<"light" | "dark">(() => {
    const mode = theme();
    if (mode === "light" || mode === "dark") return mode;
    return systemDark() ? "dark" : "light";
  });

  createEffect(() => {
    const nextToken = connectedToken();
    if (nextToken.length === 0) {
      return;
    }

    const load = () => {
      fetch(runtimeServicesUrl(nextToken))
        .then(async (response) => {
          if (!response.ok) {
            throw new Error(await response.text());
          }
          return response.json() as Promise<unknown[]>;
        })
        .then((items) => setServices(items.map(parseServiceRow)))
        .catch(() => setServices([]));
    };
    load();
    const timer = window.setInterval(load, 5000);
    onCleanup(() => window.clearInterval(timer));
  });

  createEffect(() => {
    const nextToken = connectedToken();
    if (nextToken.length === 0) {
      setDeployTargets([]);
      return;
    }
    // Deploy targets come from a registry constant — they only change on
    // jam-ui-server redeploy, so a single fetch on token connect is fine.
    fetch(deployTargetsUrl(nextToken))
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(await response.text());
        }
        return response.json() as Promise<unknown[]>;
      })
      .then((items) => setDeployTargets(items.map(parseDeployTargetRow)))
      .catch(() => setDeployTargets([]));
  });

  createEffect(() => {
    const nextToken = connectedToken();
    if (nextToken.length === 0) {
      return;
    }

    const load = () => {
      fetch(tasksUrl(nextToken))
        .then(async (response) => {
          if (!response.ok) {
            throw new Error(await response.text());
          }
          return response.json() as Promise<TaskGraphRow[]>;
        })
        .then((rows) => setTaskSnapshots(rows.map(taskRowFromGraph)))
        .catch(() => setTaskSnapshots([]));
    };
    load();
    const timer = window.setInterval(load, 15000);
    onCleanup(() => window.clearInterval(timer));
  });

  createEffect(() => {
    const nextToken = connectedToken();
    if (nextToken.length === 0) {
      return;
    }

    const load = () => {
      fetch(quotaUrl(nextToken))
        .then(async (response) => {
          if (!response.ok) {
            throw new Error(await response.text());
          }
          return response.json() as Promise<QuotaSnapshot>;
        })
        .then((snapshot) => {
          setQuotaSnapshot(snapshot);
          setQuotaError("");
          setQuotaRefreshedAt(new Date());
        })
        .catch((error: unknown) => {
          setQuotaError(error instanceof Error ? error.message : String(error));
        });
    };
    load();
    const timer = window.setInterval(load, 300000);
    onCleanup(() => window.clearInterval(timer));
  });

  createEffect(() => {
    const nextToken = connectedToken();
    if (nextToken.length === 0) {
      return;
    }

    const socket = new WebSocket(wsUrl(nextToken, "notify.human"));
    socket.addEventListener("message", (message) => {
      const parsed = parseBusEvent(message.data);
      setNotifications((items) => [notificationFromEvent(parsed), ...items].slice(0, 40));
      setDrawerOpen(true);
    });
    onCleanup(() => socket.close());
  });

  createEffect(() => {
    const traceId = traceDetailId();
    if (!traceId) {
      setTraceReplay({ status: "idle" });
      return;
    }

    const nextToken = token().trim();
    if (nextToken.length === 0) {
      setTraceReplay({ status: "error", traceId, error: "token required" });
      return;
    }

    const controller = new AbortController();
    setTraceReplay({ status: "loading", traceId });
    fetch(traceReplayUrl(nextToken, traceId), { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) {
          const detail = await response.text();
          throw new Error(detail || `${response.status} ${response.statusText}`);
        }
        return response.json() as Promise<TraceReplay>;
      })
      .then((replay) => setTraceReplay({ status: "loaded", traceId, replay }))
      .catch((error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        setTraceReplay({
          status: "error",
          traceId,
          error: error instanceof Error ? error.message : String(error)
        });
      });
    onCleanup(() => controller.abort());
  });

  onCleanup(() => streamSocket?.close());

  const connect = () => {
    const nextToken = token().trim();
    const nextSubject = subject().trim() || "journal.>";
    if (nextToken.length === 0) {
      setStatus("token required");
      return;
    }

    streamSocket?.close();
    setSubject(nextSubject);
    setConnectedToken(nextToken);
    setStatus("loading backlog");
    fetch(recentEventsUrl(nextToken, nextSubject))
      .then(async (response) => {
        if (!response.ok) {
          const detail = await response.text();
          throw new Error(detail || `${response.status} ${response.statusText}`);
        }
        return response.json() as Promise<WireBusEvent[]>;
      })
      .then((items) => {
        const parsed = items.map((item) => parseWireBusEvent(item, "recent"));
        setEvents((existing) => mergeEvents(parsed, existing));
      })
      .catch((error: unknown) => {
        setStatus(`backlog error: ${error instanceof Error ? error.message : String(error)}`);
      });

    streamSocket = new WebSocket(wsUrl(nextToken, nextSubject));
    const socket = streamSocket;
    setStatus("connecting");
    socket.addEventListener("open", () => {
      setStatus("connected");
      setLastConnectedAt(new Date());
    });
    socket.addEventListener("close", () => {
      if (streamSocket === socket) {
        setStatus("disconnected");
      }
    });
    socket.addEventListener("error", () => {
      if (streamSocket === socket) {
        setStatus("error");
      }
    });
    socket.addEventListener("message", (message) => {
      const parsed = parseBusEvent(message.data);
      setEvents((items) => mergeEvents([parsed], items));
    });
  };

  createEffect(() => {
    const nextToken = token().trim();
    if (!nextToken || nextToken === autoConnectedToken) {
      return;
    }
    if (status() !== "disconnected" && status() !== "token required") {
      return;
    }
    autoConnectedToken = nextToken;
    queueMicrotask(connect);
  });

  const disconnect = () => {
    streamSocket?.close();
    streamSocket = undefined;
    autoConnectedToken = "";
    setConnectedToken("");
    setEvents([]);
    setStatus("disconnected");
    setToken("");
    localStorage.removeItem("jam.ui.token");
  };

  const pickerRows = createMemo(() => pickerRowsFromEvents(events()));
  const traceRows = createMemo(() => traceRowsFromEvents(events()));
  const quotaRows = createMemo(() =>
    mergeQuotaRows(quotaRowsFromSnapshot(quotaSnapshot()), quotaRowsFromEvents(events()))
  );
  const taskRows = createMemo(() =>
    mergeTaskRows(
      taskSnapshots(),
      taskRowsFromEvents(events()),
      taskRowsFromEvents(events().filter((event) => event.source === "live"))
    )
  );
  const prRows = createMemo(() => prRowsFromEvents(events()));
  const maestroEvents = createMemo(() =>
    events().filter((event) => event.subject.includes(".maestro."))
  );

  return (
    <Show
      when={connectedToken().length > 0}
      fallback={
        <TokenGate
          token={token()}
          status={status()}
          onToken={setToken}
          onConnect={connect}
        />
      }
    >
    <main
      class={`min-h-screen bg-[#f7f7f8] text-[#171717] theme-${effective()}`}
      data-theme={effective()}
      data-theme-mode={theme()}
    >
      <div class="grid min-h-screen grid-cols-1 lg:grid-cols-[240px_minmax(0,1fr)]">
        <aside class="min-w-0 border-b border-[#e5e5e5] bg-[#f4f4f5] px-3 py-3 lg:border-b-0 lg:border-r">
          <div class="mb-4 flex items-center justify-between gap-3 px-2">
            <a class="text-base font-semibold tracking-normal" href="/">Jamboree</a>
            <button
              class="rounded-md border border-[#d7d7d7] bg-white px-2.5 py-1.5 text-xs hover:bg-[#eeeeef]"
              type="button"
              onClick={() => setDrawerOpen(true)}
            >
              Alerts {unreadCount()}
            </button>
          </div>
          <nav class="flex gap-1 overflow-x-auto pb-3 lg:block lg:space-y-1 lg:overflow-visible lg:pb-0">
            <For each={navItems}>
              {(item) => (
                <a
                  class="flex items-center justify-between rounded-md px-3 py-2 text-sm text-[#444444] hover:bg-[#e9e9ea]"
                  classList={{
                    "bg-white font-medium text-[#171717] shadow-sm": currentPath() === item.href
                  }}
                  href={item.href}
                >
                  <span>{item.label}</span>
                </a>
              )}
            </For>
          </nav>
          <div class="mt-4 space-y-3">
            <Show when={status() !== "connected"}>
              <ConnectionControls
                token={token()}
                status={status()}
                onToken={setToken}
                onReconnect={connect}
                onClear={disconnect}
              />
            </Show>
            <AdvancedControls
              subject={subject()}
              onSubject={setSubject}
              onApply={connect}
            />
          </div>
        </aside>

        <section class="min-w-0">
          <header class="sticky top-0 z-10 border-b border-[#e5e5e5] bg-[#f7f7f8]/95 px-4 py-3 backdrop-blur sm:px-6">
            <div class="mx-auto flex max-w-[1680px] items-center justify-between gap-4">
              <div>
                <h1 class="text-lg font-semibold">Orchestrator</h1>
                <p class="text-sm text-[#666666]">Blueberry and Jamboree work queues</p>
              </div>
              <div class="flex items-center gap-2">
                <ThemeToggle mode={theme()} effective={effective()} onChange={setTheme} />
                <div class="rounded-full border border-[#d7d7d7] bg-white px-3 py-1.5 text-sm">
                  <span class="mr-2 inline-block h-2 w-2 rounded-full bg-[#19a463]" />
                  {status()}
                </div>
              </div>
            </div>
          </header>
          <div class="mx-auto max-w-[1680px] px-4 py-5 sm:px-6">
            <ViewRouter
              path={currentPath()}
              status={status()}
              subject={subject()}
              lastConnectedAt={lastConnectedAt()}
              events={events()}
              notifications={notifications()}
              services={services()}
              taskRows={taskRows()}
              pickerRows={pickerRows()}
              prRows={prRows()}
              traceRows={traceRows()}
              quotaRows={quotaRows()}
              quotaError={quotaError()}
              quotaRefreshedAt={quotaRefreshedAt()}
              maestroEvents={maestroEvents()}
              traceReplay={traceReplay()}
              token={token()}
              createTaskState={createTaskState()}
              onCreateTaskState={setCreateTaskState}
              deployTargets={deployTargets()}
            />
          </div>
        </section>
      </div>

      <NotificationDrawer
        open={drawerOpen()}
        notifications={notifications()}
        onClose={() => setDrawerOpen(false)}
        onClear={() => setNotifications([])}
      />
    </main>
    </Show>
  );
}

function ViewRouter(props: RouteData & { path: string }) {
  return routeView(props.path, props);
}

function TokenGate(props: {
  token: string;
  status: string;
  onToken: (value: string) => void;
  onConnect: () => void;
}) {
  const errorMessage = () => {
    const s = props.status;
    if (s === "token required" || s === "disconnected" || s === "connecting" || s === "connected") {
      return "";
    }
    return s.startsWith("backlog error") || s === "error" ? s : "";
  };

  return (
    <main class="flex min-h-screen items-center justify-center bg-[#f6f6f3] px-4 py-10 text-[#161815]">
      <form
        class="w-full max-w-md rounded-lg border border-[#dddcd4] bg-white p-6 shadow-sm"
        onSubmit={(event) => {
          event.preventDefault();
          props.onConnect();
        }}
      >
        <h1 class="text-xl font-semibold">Jamboree</h1>
        <p class="mt-1 text-sm text-[#62665e]">
          Enter a session token to connect. Issue one with{" "}
          <code class="rounded bg-[#f0f0ea] px-1 py-0.5 text-xs">jam ui token</code> from the host shell.
        </p>

        <label class="mt-5 block text-sm font-medium">
          Session token
          <input
            class="mt-1 block w-full rounded-md border border-[#d3d2ca] px-3 py-2 text-sm text-[#161815] focus:border-[#284b35] focus:outline-none"
            type="password"
            autocomplete="off"
            spellcheck={false}
            autofocus
            value={props.token}
            onInput={(event) => props.onToken(event.currentTarget.value)}
          />
        </label>

        {errorMessage() && (
          <p class="mt-3 text-sm text-[#9a2b1f]">{errorMessage()}</p>
        )}

        <button
          class="mt-5 w-full rounded-md bg-[#284b35] px-4 py-2 text-sm font-medium text-white hover:bg-[#203d2b] disabled:cursor-not-allowed disabled:bg-[#9da89e]"
          type="submit"
          disabled={props.token.trim().length === 0}
        >
          Connect
        </button>
      </form>
    </main>
  );
}

function ConnectionControls(props: {
  token: string;
  status: string;
  onToken: (value: string) => void;
  onReconnect: () => void;
  onClear: () => void;
}) {
  const errorMessage = () => {
    const s = props.status;
    return s.startsWith("backlog error") || s === "error" ? s : "";
  };

  return (
    <section class="rounded-md border border-[#dddcd4] bg-white px-4 py-3">
      <div class="text-sm font-medium">Connection</div>
      <label class="mt-3 grid gap-1 text-xs text-[#62665e]">
        Session token
        <input
          class="w-full rounded-md border border-[#d3d2ca] px-3 py-2 text-sm text-[#161815]"
          type="password"
          name="jam-session-token"
          autocomplete="off"
          spellcheck={false}
          value={props.token}
          onInput={(event) => props.onToken(event.currentTarget.value)}
          placeholder="Paste jam ui token"
        />
      </label>
      <Show when={errorMessage().length > 0}>
        <div class="mt-2 break-words text-xs leading-5 text-[#9a2b1f]">{errorMessage()}</div>
      </Show>
      <div class="mt-3 flex gap-2">
        <button
          class="flex-1 rounded-md bg-[#284b35] px-3 py-2 text-sm text-white hover:bg-[#203d2b] disabled:cursor-not-allowed disabled:bg-[#9da89e]"
          type="button"
          disabled={props.token.trim().length === 0}
          onClick={props.onReconnect}
        >
          Reconnect
        </button>
        <button
          class="rounded-md border border-[#d3d2ca] px-3 py-2 text-sm hover:bg-[#f0f0ea]"
          type="button"
          onClick={props.onClear}
        >
          Clear
        </button>
      </div>
    </section>
  );
}

function AdvancedControls(props: {
  subject: string;
  onSubject: (value: string) => void;
  onApply: () => void;
}) {
  return (
    <details class="rounded-md border border-[#dddcd4] bg-white px-4 py-3">
      <summary class="cursor-pointer text-sm font-medium">Event Filter</summary>
      <div class="mt-3 grid gap-3">
        <label class="grid gap-1 text-xs text-[#62665e]">
          Live event subject
          <input
            class="w-full rounded-md border border-[#d3d2ca] px-3 py-2 text-sm text-[#161815]"
            name="jam-subject-filter"
            autocomplete="off"
            spellcheck={false}
            value={props.subject}
            onInput={(event) => props.onSubject(event.currentTarget.value)}
            placeholder="journal.>"
          />
          <span class="text-[11px] text-[#878a82]">
            Advanced NATS filter. Default <code class="rounded bg-[#f0f0ea] px-1">journal.&gt;</code> shows all journal events.
          </span>
        </label>
        <button
          class="rounded-md border border-[#d3d2ca] px-3 py-2 text-sm hover:bg-[#f0f0ea]"
          type="button"
          onClick={props.onApply}
        >
          Apply filter
        </button>
      </div>
    </details>
  );
}

function DashboardView(props: {
  status: string;
  subject: string;
  lastConnectedAt: Date | null;
  events: BusEvent[];
  notifications: HumanNotification[];
  services: ServiceRow[];
  taskRows: TaskRow[];
  pickerRows: PickerRow[];
  prRows: PrRow[];
  traceRows: TraceRow[];
  quotaRows: QuotaRow[];
  quotaError: string;
  quotaRefreshedAt: Date | null;
  token: string;
  createTaskState: CreateTaskState;
  onCreateTaskState: (state: CreateTaskState) => void;
  deployTargets: DeployTargetRow[];
}) {
  const runningServices = createMemo(() => props.services.filter((service) => service.running));
  const disabledServices = createMemo(() =>
    props.services.filter((service) => service.status.toLowerCase() === "disabled")
  );
  const serviceAttention = createMemo(() =>
    props.services.filter(
      (service) => !service.running && service.status.toLowerCase() !== "disabled"
    )
  );
  const backlogTasks = createMemo(() => props.taskRows.filter((task) => task.status === "backlog"));
  const activeTasks = createMemo(() =>
    props.taskRows.filter((task) => isInFlightTaskStatus(task.status))
  );
  const handoffTasks = createMemo(() =>
    props.taskRows.filter((task) => task.status === "picker-completed")
  );
  const failedTasks = createMemo(() => props.taskRows.filter((task) => task.status === "failed"));
  const activePrs = createMemo(() => props.prRows.filter((pr) => isOpenPrStatus(pr.status)));
  const mergedPrs = createMemo(() => props.prRows.filter((pr) => pr.status === "merged"));
  const taskStatusCounts = createMemo(() => countByStatus(props.taskRows));
  const stats = createMemo<Stat[]>(() => [
    {
      label: "System",
      value: serviceAttention().length === 0 ? "steady" : `${serviceAttention().length} attention`,
      detail: `${runningServices().length} running / ${disabledServices().length} disabled`
    },
    {
      label: "In Flight",
      value: String(activeTasks().length),
      detail: `${handoffTasks().length} ready for PR`
    },
    {
      label: "Backlog",
      value: String(backlogTasks().length),
      detail: latestTime(backlogTasks()[0]?.updatedAt)
    },
    {
      label: "PRs",
      value: String(activePrs().length),
      detail: `${mergedPrs().length} merged`
    },
    {
      label: "Failures",
      value: String(failedTasks().length),
      detail: latestTime(failedTasks()[0]?.updatedAt)
    },
    {
      label: "Quota",
      value: quotaHeadline(props.quotaRows, props.quotaError),
      detail: `refreshed ${latestTime(props.quotaRefreshedAt ?? undefined)}`
    }
  ]);

  return (
    <div class="space-y-5">
      <section class="grid gap-3 md:grid-cols-3 xl:grid-cols-6">
        <For each={stats()}>{(stat) => <StatTile stat={stat} />}</For>
      </section>

      <section class="min-w-0 grid gap-5 xl:grid-cols-[minmax(0,1.5fr)_minmax(340px,0.7fr)]">
        <div class="min-w-0 space-y-5">
          <Panel title="Pipeline">
            <div class="grid gap-4 p-4">
              <TaskStatusStrip counts={taskStatusCounts()} />
              <TaskTable rows={importantTasks(props.taskRows)} compact />
            </div>
          </Panel>
          <Show when={props.prRows.some(prIsStuck)}>
            <Panel title="PRs Needing Attention">
              <StuckPrTable rows={props.prRows.filter(prIsStuck)} />
            </Panel>
          </Show>
          <Panel title="Pull Requests">
            <PrTable rows={props.prRows.slice(0, 8)} compact />
          </Panel>
          <Panel title="Services">
            <ServiceTable rows={props.services} compact />
          </Panel>
          <DeployPanel token={props.token} targets={props.deployTargets} />
        </div>

        <div class="min-w-0 space-y-5">
          <TaskComposer
            token={props.token}
            state={props.createTaskState}
            onState={props.onCreateTaskState}
          />
          <Panel title="Quota">
            <QuotaSnapshotPanel
              rows={props.quotaRows}
              error={props.quotaError}
              refreshedAt={props.quotaRefreshedAt}
            />
          </Panel>
          <Panel title="Live Feed">
            <EventList events={props.events.slice(0, 5)} empty="No events received." />
          </Panel>
        </div>
      </section>
    </div>
  );
}

function TaskComposer(props: {
  token: string;
  state: CreateTaskState;
  onState: (state: CreateTaskState) => void;
}) {
  const [description, setDescription] = createSignal("");
  const [target, setTarget] = createSignal<TaskTarget>("blueberry");
  const [taskClass, setTaskClass] = createSignal("light-edit");
  const [priority, setPriority] = createSignal("normal");
  const taskClassOptions = createMemo(() =>
    target() === "jamboree"
      ? ["jamboree-self-modification", "investigation", "medium-edit", "doc-generation"]
      : ["light-edit", "medium-edit", "investigation", "compile-heavy-rust", "ecs-refactor"]
  );

  const chooseTarget = (next: TaskTarget) => {
    setTarget(next);
    setTaskClass(next === "jamboree" ? "jamboree-self-modification" : "light-edit");
  };

  const submit = async () => {
    const body = description().trim();
    if (!props.token.trim()) {
      props.onState({ status: "error", message: "token required" });
      return;
    }
    if (!body) {
      props.onState({ status: "error", message: "description required" });
      return;
    }
    props.onState({ status: "submitting" });
    try {
      const response = await fetch(tasksUrl(props.token.trim()), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          description: body,
          project: target(),
          task_class: taskClass(),
          priority: priority()
        })
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      const created = (await response.json()) as { task_id?: string; trace_id?: string };
      props.onState({
        status: "created",
        taskId: created.task_id ?? "-",
        traceId: created.trace_id ?? "-"
      });
      setDescription("");
    } catch (error: unknown) {
      props.onState({ status: "error", message: error instanceof Error ? error.message : String(error) });
    }
  };

  return (
    <Panel title="New Task">
      <div class="grid gap-4 p-4">
        <div class="grid gap-2">
          <div class="text-xs font-medium text-[#666666]">Target</div>
          <div class="grid gap-2 sm:grid-cols-2">
            <TaskTargetRadio
              checked={target() === "blueberry"}
              label="Blueberry"
              detail="Game code and Tempyr task work"
              onSelect={() => chooseTarget("blueberry")}
            />
            <TaskTargetRadio
              checked={target() === "jamboree"}
              label="Jamboree"
              detail="Self-modify the orchestrator"
              onSelect={() => chooseTarget("jamboree")}
            />
          </div>
        </div>
        <label class="grid gap-1 text-xs text-[#62665e]">
          Task
          <textarea
            class="min-h-32 w-full resize-y rounded-xl border border-[#d7d7d7] bg-white px-4 py-3 text-sm leading-6 text-[#171717] outline-none focus:border-[#9b9b9b] focus:ring-2 focus:ring-[#e8e8e8]"
            value={description()}
            onInput={(event) => setDescription(event.currentTarget.value)}
            placeholder={
              target() === "jamboree"
                ? "Change the orchestrator, UI, services, docs, deploy flow, or integrations..."
                : "Describe the Blueberry change or investigation..."
            }
          />
        </label>
        <div class="grid gap-3 sm:grid-cols-2">
          <label class="grid gap-1 text-xs text-[#62665e]">
            Class
            <select
              class="min-w-0 rounded-md border border-[#d7d7d7] bg-white px-3 py-2 text-sm text-[#171717] outline-none focus:border-[#9b9b9b]"
              value={taskClass()}
              onChange={(event) => setTaskClass(event.currentTarget.value)}
            >
              <For each={taskClassOptions()}>
                {(option) => <option value={option}>{option}</option>}
              </For>
            </select>
          </label>
          <label class="grid gap-1 text-xs text-[#62665e]">
            Priority
            <select
              class="min-w-0 rounded-md border border-[#d7d7d7] bg-white px-3 py-2 text-sm text-[#171717] outline-none focus:border-[#9b9b9b]"
              value={priority()}
              onChange={(event) => setPriority(event.currentTarget.value)}
            >
              <option value="normal">normal</option>
              <option value="low">low</option>
              <option value="high">high</option>
            </select>
          </label>
        </div>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <CreateTaskFeedback state={props.state} />
          <button
            class="rounded-full bg-[#171717] px-5 py-2 text-sm font-medium text-white hover:bg-[#333333] disabled:cursor-not-allowed disabled:bg-[#a3a3a3]"
            type="button"
            disabled={props.state.status === "submitting"}
            onClick={submit}
          >
            {props.state.status === "submitting" ? "Adding" : "Add Task"}
          </button>
        </div>
      </div>
    </Panel>
  );
}

function DeployPanel(props: { token: string; targets: DeployTargetRow[] }) {
  const [state, setState] = createSignal<DeployState>({ status: "idle" });
  const sorted = createMemo(() => {
    // Same order as the registry returns. No additional sort — that order is
    // already chosen (svc services, singletons, maestro, cli).
    return props.targets;
  });
  const deploy = async (service: string) => {
    if (!props.token.trim()) {
      setState({ status: "error", service, message: "token required" });
      return;
    }
    setState({ status: "deploying", service });
    try {
      const url = new URL("/api/deploy", window.location.href);
      url.searchParams.set("token", props.token.trim());
      const response = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ service })
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      const result = (await response.json()) as {
        service?: string;
        version?: string;
        outcome?: string;
        detail?: string;
        trace_id?: string;
      };
      if (result.outcome !== "confirmed") {
        throw new Error(result.detail ?? `outcome=${result.outcome ?? "unknown"}`);
      }
      setState({
        status: "ok",
        service: result.service ?? service,
        version: result.version ?? "-",
        detail: result.detail ?? "",
        traceId: result.trace_id ?? "-"
      });
    } catch (err: unknown) {
      setState({
        status: "error",
        service,
        message: err instanceof Error ? err.message : String(err)
      });
    }
  };
  return (
    <Panel title="Deploy">
      <div class="grid gap-3 p-4">
        <p class="text-xs text-[#62665e]">
          Publishes <code>patch.staged</code> for the selected component. Build the
          binary first (<code>cargo build --release -p &lt;crate&gt;</code> on the
          monorepo host) — the registered staging path is read by the server.
        </p>
        <Show
          when={sorted().length > 0}
          fallback={<EmptyState text="No deploy targets visible (token expired?)." />}
        >
          <div class="max-w-full overflow-x-auto">
            <table class="w-full min-w-[420px] border-collapse text-left text-sm">
              <thead class="border-b border-[#d7ddce] text-xs uppercase text-[#5b6558]">
                <tr>
                  <th class="px-3 py-2 font-medium">Service</th>
                  <th class="px-3 py-2 font-medium">Strategy</th>
                  <th class="px-3 py-2 font-medium"></th>
                </tr>
              </thead>
              <tbody class="divide-y divide-[#edf0e8]">
                <For each={sorted()}>
                  {(row) => {
                    const isInflight = () => {
                      const s = state();
                      return s.status === "deploying" && s.service === row.shortName;
                    };
                    return (
                      <tr>
                        <td class="px-3 py-2 font-medium">{row.shortName}</td>
                        <td class="px-3 py-2 text-xs text-[#5b6558]">{row.strategy}</td>
                        <td class="px-3 py-2 text-right">
                          <button
                            type="button"
                            class="rounded-md border border-[#d7ddce] bg-white px-3 py-1 text-xs hover:bg-[#f1f4ec] disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={state().status === "deploying"}
                            onClick={() => {
                              void deploy(row.shortName);
                            }}
                            title={`Deploy ${row.shortName} via ${row.strategy}`}
                          >
                            {isInflight() ? "Deploying…" : "Deploy"}
                          </button>
                        </td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
        <Show when={state().status === "ok"}>
          {(_ok) => {
            const s = state();
            if (s.status !== "ok") return null;
            return (
              <div class="rounded-md border border-[#9ebf8a] bg-[#f1f8ea] p-3 text-xs">
                <div class="font-medium text-[#284b35]">
                  {s.service} confirmed ({s.version})
                </div>
                <div class="mt-1 text-[#5b6558]">{s.detail}</div>
                <div class="mt-1 text-[#5b6558]">trace {s.traceId}</div>
              </div>
            );
          }}
        </Show>
        <Show when={state().status === "error"}>
          {(_err) => {
            const s = state();
            if (s.status !== "error") return null;
            return (
              <div class="rounded-md border border-[#d3504f] bg-[#fbeded] p-3 text-xs text-[#7a2a29]">
                <div class="font-medium">{s.service} failed</div>
                <div class="mt-1 whitespace-pre-wrap">{s.message}</div>
              </div>
            );
          }}
        </Show>
      </div>
    </Panel>
  );
}

function TaskTargetRadio(props: {
  checked: boolean;
  label: string;
  detail: string;
  onSelect: () => void;
}) {
  return (
    <label
      class="grid cursor-pointer grid-cols-[auto_minmax(0,1fr)] gap-3 rounded-lg border bg-white p-3 text-sm"
      classList={{
        "border-[#171717] shadow-sm": props.checked,
        "border-[#d7d7d7] hover:border-[#b8b8b8]": !props.checked
      }}
    >
      <input
        class="mt-1 h-4 w-4 accent-[#171717]"
        type="radio"
        name="task-target"
        checked={props.checked}
        onChange={props.onSelect}
      />
      <span class="min-w-0">
        <span class="block font-medium text-[#171717]">{props.label}</span>
        <span class="mt-0.5 block text-xs text-[#666666]">{props.detail}</span>
      </span>
    </label>
  );
}

function CreateTaskFeedback(props: { state: CreateTaskState }) {
  if (props.state.status === "idle") {
    return <div class="text-sm text-[#62665e]" />;
  }
  if (props.state.status === "submitting") {
    return <div class="text-sm text-[#62665e]">Submitting</div>;
  }
  if (props.state.status === "created") {
    return (
      <div class="min-w-0 text-sm text-[#284b35]">
        <span class="font-medium">Requested</span>
        <span class="ml-2 break-all text-xs text-[#62665e]">{props.state.taskId}</span>
      </div>
    );
  }
  return <ErrorBlock text={props.state.message} />;
}

function TaskStatusStrip(props: { counts: Map<string, number> }) {
  const items = [
    "backlog",
    "running",
    "picker-completed",
    "draft",
    "in-review",
    "merged",
    "failed"
  ];
  return (
    <div class="grid grid-cols-2 gap-2 sm:grid-cols-4 xl:grid-cols-7">
      <For each={items}>
        {(status) => (
          <div class="rounded-md border border-[#e0dfd7] bg-[#fafaf7] px-3 py-2">
            <div class="flex items-center justify-between gap-2">
              <div class="text-lg font-semibold">{props.counts.get(status) ?? 0}</div>
              <StatusSymbol status={status} compact />
            </div>
            <div class="mt-1 truncate text-xs text-[#62665e]">{statusLabel(status)}</div>
          </div>
        )}
      </For>
    </div>
  );
}

function QuotaSnapshotPanel(props: { rows: QuotaRow[]; error: string; refreshedAt: Date | null }) {
  return (
    <div class="grid gap-3 p-4">
      <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-[#62665e]">
        <span>{props.rows.length} windows</span>
        <span>{latestTime(props.refreshedAt ?? undefined)}</span>
      </div>
      <Show when={props.error.length > 0}>
        <ErrorBlock text={props.error} />
      </Show>
      <Show when={props.rows.length > 0} fallback={<EmptyState text="No quota states." />}>
        <div class="grid gap-3">
          <For each={props.rows.slice(0, 5)}>
            {(row) => (
              <article class="grid gap-2 rounded-md border border-[#e0dfd7] bg-[#fafaf7] p-3">
                <div class="flex min-w-0 items-center justify-between gap-3">
                  <div class="min-w-0 truncate text-sm font-medium">{row.key}</div>
                  <StatusPill status={row.status} />
                </div>
                <div class="h-2 overflow-hidden rounded-full bg-[#e6e5dd]">
                  <div
                    class="h-full rounded-full bg-[#516a98]"
                    style={{ width: `${remainingPercent(row.remaining)}%` }}
                  />
                </div>
                <div class="grid gap-1 text-xs text-[#62665e]">
                  <div>{row.remaining} remaining</div>
                  <div class="truncate">{row.usage}</div>
                  <Show when={row.detail !== "-"}>
                    <div class="truncate">{row.detail}</div>
                  </Show>
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function TasksView(props: {
  rows: TaskRow[];
  token: string;
  createTaskState: CreateTaskState;
  onCreateTaskState: (state: CreateTaskState) => void;
}) {
  const [showStale, setShowStale] = createSignal(false);
  const visible = createMemo(() =>
    showStale() ? props.rows : props.rows.filter((row) => !isStaleTask(row))
  );
  const hiddenCount = createMemo(() => props.rows.length - visible().length);
  return (
    <div class="min-w-0 grid gap-5 xl:grid-cols-[minmax(0,1fr)_380px]">
      <Panel title="Tasks">
        <Show when={hiddenCount() > 0}>
          <div class="flex items-center justify-between border-b border-[#ecebe3] px-4 py-2 text-xs text-[#62665e]">
            <span>
              {hiddenCount()} stale task{hiddenCount() === 1 ? "" : "s"} hidden
              <span class="ml-1 text-[#9aa195]">
                (failed/merged/picker-completed &gt; 24h or older than 7 days)
              </span>
            </span>
            <button
              type="button"
              class="rounded-md border border-[#d7ddce] bg-white px-2 py-0.5 hover:bg-[#f1f4ec]"
              onClick={() => setShowStale((v) => !v)}
            >
              {showStale() ? "Hide stale" : "Show all"}
            </button>
          </div>
        </Show>
        <TaskTable rows={visible()} />
      </Panel>
      <TaskComposer
        token={props.token}
        state={props.createTaskState}
        onState={props.onCreateTaskState}
      />
    </div>
  );
}

/// Treat a task as "stale" (auto-hidden from the full Tasks page) when it's
/// terminal AND older than the per-status cutoff. The dashboard's Pipeline
/// panel always uses `importantTasks` so live + recently-failed tasks still
/// surface there; this only affects the long list view.
function isStaleTask(row: TaskRow): boolean {
  const ageMs = Date.now() - row.updatedAt.getTime();
  const ONE_DAY = 24 * 60 * 60 * 1000;
  const SEVEN_DAYS = 7 * ONE_DAY;
  if (row.status === "failed" || row.status === "abandoned" || row.status === "merged") {
    return ageMs > ONE_DAY;
  }
  // `picker-completed` means the picker exited cleanly but post-picker
  // never moved the task forward (open-pr failed, continuation cap fired
  // before task.failed was emitted, etc.). Anything still stuck there
  // after a day is a dead task that won't recover on its own — hide it
  // so the dashboard's "ready for PR" list reflects actually-pending PRs.
  if (row.status === "picker-completed") {
    return ageMs > ONE_DAY;
  }
  // Older-than-7d catches "backlog" entries that never got picked up plus
  // anything else that fell out of view. Active in-flight tasks should
  // refresh their updated_at on every event, so they won't trip this.
  return ageMs > SEVEN_DAYS;
}

function TaskDetailView(props: {
  taskId: string;
  rows: TaskRow[];
  events: BusEvent[];
  prs: PrRow[];
  pickers: PickerRow[];
  token: string;
}) {
  const task = createMemo(() => props.rows.find((row) => row.taskId === props.taskId));
  const taskEvents = createMemo(() =>
    props.events.filter((event) => eventTouchesTask(event, props.taskId))
  );
  const taskPrs = createMemo(() => props.prs.filter((pr) => pr.taskId === props.taskId));
  const taskPickers = createMemo(() =>
    props.pickers.filter((picker) => picker.taskId === props.taskId)
  );
  const currentSessionId = createMemo(
    () => taskPickers()[0]?.sessionId ?? task()?.sessionId ?? ""
  );
  const timelineItems = createMemo(() => taskTimelineItems(taskEvents()));
  const latestItem = createMemo(() => timelineItems()[0]);

  return (
    <div class="min-w-0 space-y-5">
      <Panel title={task() ? taskDisplayName(task() as TaskRow) : "Task"}>
        <div class="grid gap-4 p-4">
          <Show when={task()} fallback={<EmptyState text="Task not found in current backlog." />}>
            {(row) => (
              <>
                <div class="grid gap-4 md:grid-cols-[auto_minmax(0,1fr)_auto] md:items-start">
                  <StatusSymbol status={row().status} />
                  <div class="min-w-0 space-y-2">
                    <h2 class="text-xl font-semibold">{taskDisplayName(row())}</h2>
                    <p class="max-w-3xl text-sm leading-6 text-[#3f443c]">
                      {taskUsefulDescription(row())}
                    </p>
                    <details class="text-xs text-[#62665e]">
                      <summary class="cursor-pointer">Technical ID</summary>
                      <div class="mt-1 break-all">{row().taskId}</div>
                    </details>
                  </div>
                  <StatusPill status={row().status} />
                </div>
                <div class="rounded-md border border-[#e0dfd7] bg-[#fafaf7] p-3">
                  <div class="text-xs uppercase text-[#62665e]">Latest Update</div>
                  <Show
                    when={latestItem()}
                    fallback={<div class="mt-1 text-sm text-[#62665e]">No progress events yet.</div>}
                  >
                    {(item) => (
                      <div class="mt-2 flex gap-3">
                        <StatusSymbol status={item().status} />
                        <div class="min-w-0">
                          <div class="font-medium">{item().title}</div>
                          <div class="mt-1 text-sm text-[#3f443c]">{item().detail}</div>
                          <div class="mt-1 text-xs text-[#62665e]">
                            {latestTime(item().updatedAt)} / {actorLabel(item().actor)}
                          </div>
                        </div>
                      </div>
                    )}
                  </Show>
                </div>
                <div class="grid gap-3 text-sm sm:grid-cols-2 xl:grid-cols-4">
                  <KeyValue label="Project" value={row().project} />
                  <KeyValue label="Class" value={row().taskClass} />
                  <KeyValue label="Priority" value={row().priority} />
                  <KeyValue label="Updated" value={latestTime(row().updatedAt)} />
                  <KeyValueCustom label="PR">
                    <PrLink prRef={row().prRef} />
                  </KeyValueCustom>
                  <KeyValue label="Requested" value={actorLabel(row().requestedBy)} />
                </div>
              </>
            )}
          </Show>
        </div>
      </Panel>
      <div class="min-w-0 grid gap-5 xl:grid-cols-2">
        <Panel title="Updates">
          <TaskTimeline items={timelineItems()} />
        </Panel>
        <div class="min-w-0 space-y-5">
          <Panel title="Run Log">
            <RunLogPanel
              token={props.token}
              taskId={task()?.taskId ?? props.taskId}
              project={usefulField(task()?.project ?? "", "blueberry")}
              harness={usefulField(
                task()?.harness ?? "",
                taskPickers()[0]?.harness ?? "codex-cli"
              )}
              taskClass={usefulField(task()?.taskClass ?? "", "light-edit")}
              taskStatus={task()?.status ?? "unknown"}
              pickerStatus={taskPickers()[0]?.status}
              sessionId={currentSessionId()}
            />
          </Panel>
          <Panel title="Pickers">
            <PickerMiniList rows={taskPickers()} />
          </Panel>
          <Panel title="Pull Requests">
            <PrTable rows={taskPrs()} compact />
          </Panel>
        </div>
      </div>
    </div>
  );
}

function TaskTimeline(props: { items: TimelineItem[] }) {
  return (
    <div class="divide-y divide-[#ecebe3]">
      <Show when={props.items.length > 0} fallback={<EmptyState text="No progress events." />}>
        <For each={props.items}>
          {(item) => (
            <article class="grid gap-2 px-4 py-3">
              <div class="flex gap-3">
                <StatusSymbol status={item.status} />
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-start justify-between gap-2">
                    <div class="font-medium">{item.title}</div>
                    <div class="text-xs text-[#62665e]">{latestTime(item.updatedAt)}</div>
                  </div>
                  <div class="mt-1 text-sm leading-6 text-[#3f443c]">{item.detail}</div>
                  <Show when={item.actor !== "-"}>
                    <div class="mt-1 text-xs text-[#62665e]">Actor: {actorLabel(item.actor)}</div>
                  </Show>
                </div>
              </div>
            </article>
          )}
        </For>
      </Show>
    </div>
  );
}

function RunLogPanel(props: {
  token: string;
  taskId: string;
  project: string;
  harness: string;
  taskClass: string;
  taskStatus: string;
  pickerStatus?: string;
  sessionId: string;
}) {
  const [records, setRecords] = createSignal<SessionOutputRecord[]>([]);
  const [status, setStatus] = createSignal("waiting for session");
  const [prompt, setPrompt] = createSignal("");
  const [mode, setMode] = createSignal<MessageMode>("queue");
  const [sendError, setSendError] = createSignal("");
  const [outbox, setOutbox] = createSignal<MessageOutboxItem[]>([]);
  const resumePreferred = createMemo(() => shouldResumePicker(props.taskStatus, props.pickerStatus));
  let logViewport: HTMLDivElement | undefined;

  createEffect(() => {
    const sessionId = props.sessionId;
    const token = props.token.trim();
    if (!sessionId || sessionId === "-") {
      setRecords([]);
      setStatus("waiting for session");
      return;
    }
    if (!token) {
      setRecords([]);
      setStatus("token required");
      return;
    }

    let closed = false;
    setStatus("loading backlog");
    fetch(sessionOutputUrl(token, sessionId))
      .then(async (response) => {
        if (!response.ok) {
          const detail = await response.text();
          throw new Error(detail || `${response.status} ${response.statusText}`);
        }
        return response.json() as Promise<unknown[]>;
      })
      .then((items) => {
        if (!closed) {
          setRecords((existing) =>
            mergeOutputRecords(items.map(parseSessionOutputRecord), existing)
          );
          setStatus("live");
        }
      })
      .catch((error: unknown) => {
        if (!closed) {
          setStatus(`backlog error: ${error instanceof Error ? error.message : String(error)}`);
        }
      });

    const socket = new WebSocket(wsUrl(token, `picker.${sessionId}.output`));
    socket.addEventListener("open", () => {
      if (!closed) {
        setStatus("live");
      }
    });
    socket.addEventListener("close", () => {
      if (!closed) {
        setStatus("disconnected");
      }
    });
    socket.addEventListener("error", () => {
      if (!closed) {
        setStatus("stream error");
      }
    });
    socket.addEventListener("message", (message) => {
      const parsed = parseBusEvent(message.data);
      setRecords((items) => mergeOutputRecords([outputRecordFromEvent(parsed)], items));
    });
    onCleanup(() => {
      closed = true;
      socket.close();
    });
  });

  createEffect(() => {
    const sessionId = props.sessionId;
    const token = props.token.trim();
    if (!sessionId || sessionId === "-" || !token) {
      return;
    }

    const socket = new WebSocket(wsUrl(token, `picker.${sessionId}.msg.status`));
    socket.addEventListener("message", (message) => {
      const parsed = parseBusEvent(message.data);
      const messageId = stringField(parsed.envelope, "message_id");
      const nextStatus = stringField(parsed.envelope, "status");
      if (!messageId || !nextStatus) {
        return;
      }
      setOutbox((items) => upsertOutboxStatus(items, messageId, nextStatus, parsed));
    });
    onCleanup(() => socket.close());
  });

  createEffect(() => {
    const timer = window.setInterval(() => {
      const now = Date.now();
      setOutbox((items) =>
        items.map((item) =>
          isPendingDeliveryStatus(item.status) && now - item.updatedAt.getTime() > 10000
            ? {
                ...item,
                status: "delivery-failed",
                detail:
                  "No delivery confirmation arrived. The Picker session may have already exited.",
                updatedAt: new Date()
              }
            : item
        )
      );
    }, 1000);
    onCleanup(() => window.clearInterval(timer));
  });

  createEffect(() => {
    records().length;
    outbox().length;
    queueMicrotask(() => {
      if (logViewport) {
        logViewport.scrollTop = logViewport.scrollHeight;
      }
    });
  });

  const sendPrompt = async () => {
    const bodyText = prompt().trim();
    const token = props.token.trim();
    const sessionId = (props.sessionId ?? "").trim();
    setSendError("");
    if (!token) {
      setSendError("token required");
      return;
    }
    if (!sessionId || sessionId === "-") {
      setSendError("session required");
      return;
    }
    if (!bodyText) {
      setSendError("message required");
      return;
    }

    const pendingId = `pending-${Date.now()}`;
    const pending: MessageOutboxItem = {
      messageId: pendingId,
      sessionId,
      mode: resumePreferred() ? "queue" : mode(),
      status: resumePreferred() ? "resuming" : "sending",
      text: bodyText,
      detail: "",
      updatedAt: new Date()
    };
    setOutbox((items) => [...items, pending].slice(-12));
    setPrompt("");

    try {
      const response = await fetch(
        resumePreferred()
          ? taskResumeUrl(token, props.taskId)
          : sessionMessageUrl(token, sessionId),
        {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: resumePreferred()
          ? JSON.stringify({
              prompt: bodyText,
              project: props.project,
              harness: props.harness,
              parent_session_id: sessionId,
              task_class: props.taskClass
            })
          : JSON.stringify({ mode: mode(), text: bodyText })
        }
      );
      if (!response.ok) {
        throw new Error(await response.text());
      }
      const rendered = (await response.json()) as unknown;
      if (resumePreferred()) {
        const resumed = recordFromUnknown(rendered);
        setOutbox((items) =>
          items.map((item) =>
            item.messageId === pendingId
              ? {
                  ...item,
                  messageId: stringField(resumed, "session_id") ?? pendingId,
                  sessionId: stringField(resumed, "session_id") ?? sessionId,
                  status: "running",
                  detail: "Started a resumed Picker for this task.",
                  updatedAt: new Date()
                }
              : item
          )
        );
      } else {
        setOutbox((items) =>
          reconcilePromptAck(items, pendingId, rendered as MessageResponse, bodyText)
        );
      }
    } catch (caught: unknown) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setSendError(message);
      setOutbox((items) =>
        items.map((item) =>
          item.messageId === pendingId
            ? { ...item, status: "delivery-failed", detail: message, updatedAt: new Date() }
            : item
        )
      );
    }
  };

  return (
    <div class="grid min-h-[620px] grid-rows-[auto_minmax(0,1fr)_auto]">
      <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[#ecebe3] px-4 py-3 text-xs text-[#62665e]">
        <span class="min-w-0 truncate">{props.sessionId && props.sessionId !== "-" ? props.sessionId : "No picker session"}</span>
        <StatusPill status={status() === "live" ? "running" : status()} />
      </div>
      <div
        ref={logViewport}
        data-testid="run-log-scroll"
        class="min-h-0 max-h-[620px] overflow-y-auto bg-[#fcfcfb] px-3 py-3"
      >
        <Show
          when={records().length > 0}
          fallback={
            <EmptyState
              text={
                props.sessionId && props.sessionId !== "-"
                  ? "No output captured for this session yet."
                  : "The run log appears after a Picker starts."
              }
            />
          }
        >
          <For each={records()}>
            {(record) => <LogLineView record={record} />}
          </For>
        </Show>
        <For each={outbox()}>
          {(item) => <PendingPrompt item={item} />}
        </For>
      </div>
      <div class="border-t border-[#ecebe3] bg-white p-3">
        <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
          <Show
            when={!resumePreferred()}
            fallback={<div class="text-xs text-[#666666]">No running Picker; this will resume the task.</div>}
          >
            <div class="inline-grid overflow-hidden rounded-md border border-[#d7d7d7] text-xs">
              <For each={(["queue", "interrupt"] as MessageMode[])}>
                {(item) => (
                  <button
                    class="px-3 py-1.5 hover:bg-[#eeeeef]"
                    classList={{
                      "bg-[#eeeeef] font-medium text-[#171717]": mode() === item
                    }}
                    type="button"
                    onClick={() => setMode(item)}
                  >
                    {modeLabel(item)}
                  </button>
                )}
              </For>
            </div>
          </Show>
          <Show when={sendError().length > 0}>
            <div class="break-words text-xs text-[#9a2b1f]">{sendError()}</div>
          </Show>
        </div>
        <div class="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
          <textarea
            aria-label="Run log prompt"
            class="min-h-20 w-full resize-y rounded-lg border border-[#d7d7d7] bg-white px-3 py-2 text-sm leading-6 text-[#171717] outline-none focus:border-[#9b9b9b] focus:ring-2 focus:ring-[#e8e8e8]"
            value={prompt()}
            onInput={(event) => setPrompt(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void sendPrompt();
              }
            }}
            placeholder="Type a prompt, e.g. continue and create the PR"
                  disabled={!props.sessionId || props.sessionId === "-"}
            title={resumePreferred() ? "This task has no running Picker; sending will resume it." : undefined}
          />
          <button
            class="rounded-full bg-[#171717] px-5 py-2 text-sm font-medium text-white hover:bg-[#333333] disabled:cursor-not-allowed disabled:bg-[#a3a3a3]"
            type="button"
            disabled={!prompt().trim() || !props.sessionId || props.sessionId === "-"}
            onClick={() => void sendPrompt()}
          >
            {resumePreferred() ? "Resume Picker" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}

function LogLineView(props: { record: SessionOutputRecord }) {
  const parsed = createMemo(() => parseLogLine(props.record));
  const isUser = createMemo(() => parsed().kind === "user");
  return (
    <article
      class="grid gap-2 px-1 py-2"
      classList={{
        "justify-items-end": isUser()
      }}
    >
      <div
        class="flex w-full max-w-[min(52rem,100%)] items-center justify-between gap-3 text-xs text-[#777777]"
        classList={{ "justify-self-end": isUser() }}
      >
        <div
          class={
            props.record.stream === "stderr"
              ? "font-medium text-[#a23a2a]"
              : "font-medium text-[#2f6f4f]"
          }
        >
          {props.record.stream}
        </div>
        <time class="text-right" dateTime={props.record.ts.toISOString()}>
          {runLogTimestamp(props.record.ts)}
        </time>
      </div>
      <div
        class="min-w-0 max-w-[min(52rem,100%)] rounded-lg border bg-white p-3 shadow-[0_1px_1px_rgba(23,23,23,0.03)]"
        classList={{
          "border-[#f0c9c4]": parsed().kind === "error",
          "border-[#d9e1f7]": parsed().kind === "assistant",
          "border-[#d7d7d7] bg-[#f4f4f5]": parsed().kind === "user",
          "border-[#d9eadf]": parsed().kind === "tool",
          "border-[#e6e6e6]": parsed().kind !== "error" && parsed().kind !== "assistant" && parsed().kind !== "user" && parsed().kind !== "tool"
        }}
      >
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0 font-medium text-[#202020]">{parsed().title}</div>
          <Show when={props.record.truncated}>
            <span class="rounded-full bg-[#fff2d8] px-2 py-0.5 text-xs text-[#7a4d00]">truncated</span>
          </Show>
        </div>
        <Show when={parsed().body.length > 0}>
          <div class="mt-2 whitespace-pre-wrap break-words text-sm leading-6 text-[#333333]">
            {parsed().body}
          </div>
        </Show>
        <Show when={parsed().meta.length > 0}>
          <div class="mt-3 flex flex-wrap gap-2">
            <For each={parsed().meta}>
              {(item) => (
                <span class="rounded-full border border-[#e1e1e1] bg-[#f8f8f8] px-2 py-0.5 text-xs text-[#666666]">
                  {item}
                </span>
              )}
            </For>
          </div>
        </Show>
      </div>
    </article>
  );
}

function PendingPrompt(props: { item: MessageOutboxItem }) {
  const detail = createMemo(() => conciseDetail(props.item.detail));
  return (
    <article class="grid justify-items-end gap-2 px-1 py-2">
      <div class="flex w-full max-w-[min(52rem,100%)] items-center justify-between gap-3 text-xs text-[#777777]">
        <div class="font-medium text-[#444444]">you</div>
        <time class="text-right" dateTime={props.item.updatedAt.toISOString()}>
          {runLogTimestamp(props.item.updatedAt)}
        </time>
      </div>
      <div class="min-w-0 max-w-[min(52rem,100%)] rounded-lg border border-[#d7d7d7] bg-[#f4f4f5] p-3 shadow-[0_1px_1px_rgba(23,23,23,0.03)]">
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0 font-medium text-[#202020]">You</div>
          <StatusPill status={props.item.status} />
        </div>
        <div class="mt-2 whitespace-pre-wrap break-words text-sm leading-6 text-[#333333]">
          {props.item.text}
        </div>
        <Show when={detail().length > 0}>
          <div class="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-[#666666]">
            {detail()}
          </div>
        </Show>
        <div class="mt-3 flex flex-wrap gap-2">
          <span class="rounded-full border border-[#e1e1e1] bg-[#f8f8f8] px-2 py-0.5 text-xs text-[#666666]">
            {modeLabel(props.item.mode)}
          </span>
        </div>
      </div>
    </article>
  );
}

function PickerMiniList(props: { rows: PickerRow[] }) {
  return (
    <Show when={props.rows.length > 0} fallback={<EmptyState text="No picker sessions." />}>
      <div class="divide-y divide-[#ecebe3]">
        <For each={props.rows}>
          {(row) => (
            <article class="grid gap-1 px-4 py-3 text-sm">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="min-w-0 break-all font-medium">{row.sessionId}</div>
                <StatusPill status={row.status} />
              </div>
              <div class="truncate text-xs text-[#62665e]">{row.harness}</div>
              <div class="truncate text-xs text-[#62665e]">{row.worktreePath}</div>
            </article>
          )}
        </For>
      </div>
    </Show>
  );
}

function PullRequestsView(props: { rows: PrRow[] }) {
  return (
    <Panel title="Pull Requests">
      <PrTable rows={props.rows} />
    </Panel>
  );
}

function PickersView(props: { rows: PickerRow[]; events: BusEvent[]; token: string }) {
  return (
    <div class="min-w-0 space-y-5">
      <Panel title="Pickers">
        <Show when={props.rows.length > 0} fallback={<EmptyState text="No picker sessions." />}>
          <div class="max-w-full overflow-x-auto">
            <table class="w-full min-w-[720px] border-collapse text-left text-sm">
              <thead class="border-b border-[#d7ddce] text-xs uppercase text-[#5b6558]">
                <tr>
                  <th class="px-4 py-2 font-medium">Session</th>
                  <th class="px-4 py-2 font-medium">Task</th>
                  <th class="px-4 py-2 font-medium">Harness</th>
                  <th class="px-4 py-2 font-medium">Status</th>
                  <th class="px-4 py-2 font-medium">Updated</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-[#edf0e8]">
                <For each={props.rows}>
                  {(row) => (
                    <tr>
                      <td class="max-w-64 truncate px-4 py-3 font-medium">{row.sessionId}</td>
                      <td class="px-4 py-3">{titleFromTaskId(row.taskId)}</td>
                      <td class="px-4 py-3">{row.harness}</td>
                      <td class="px-4 py-3">
                        <StatusPill status={row.status} />
                      </td>
                      <td class="px-4 py-3 text-[#5b6558]">{latestTime(row.updatedAt)}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Panel>
      <PickerMessageComposer rows={props.rows} events={props.events} token={props.token} />
    </div>
  );
}

function PickerMessageComposer(props: { rows: PickerRow[]; events: BusEvent[]; token: string }) {
  const [sessionId, setSessionId] = createSignal("");
  const [mode, setMode] = createSignal<MessageMode>("queue");
  const [text, setText] = createSignal("");
  const [error, setError] = createSignal("");
  const [outbox, setOutbox] = createSignal<MessageOutboxItem[]>([]);

  createEffect(() => {
    if (sessionId().length === 0 && props.rows.length > 0) {
      setSessionId(props.rows[0].sessionId);
    }
  });

  createEffect(() => {
    for (const event of props.events) {
      if (!event.subject.endsWith(".msg.status")) {
        continue;
      }
      const id = stringField(event.envelope, "message_id");
      const nextStatus = stringField(event.envelope, "status");
      if (!id || !nextStatus) {
        continue;
      }
      setOutbox((items) => upsertOutboxStatus(items, id, nextStatus, event));
    }
  });

  const chooseMode = (next: MessageMode) => {
    if (next === "full-stop" && mode() !== "full-stop") {
      const selected = sessionId() || "selected Picker";
      if (!window.confirm(`Kill ${selected}? Worktree will be preserved.`)) {
        return;
      }
    }
    setMode(next);
  };

  const send = async () => {
    setError("");
    const selected = sessionId().trim();
    const bodyText = text().trim();
    if (!props.token.trim()) {
      setError("token required");
      return;
    }
    if (!selected) {
      setError("session required");
      return;
    }
    if (!bodyText) {
      setError(mode() === "full-stop" ? "reason required" : "message required");
      return;
    }

    const pendingStatus = mode() === "full-stop" ? "kill-requested" : "sending";
    const pendingId = `pending-${Date.now()}`;
    setOutbox((items) =>
      [
        {
          messageId: pendingId,
          sessionId: selected,
          mode: mode(),
          status: pendingStatus,
          text: bodyText,
          detail: "",
          updatedAt: new Date()
        },
        ...items
      ].slice(0, 12)
    );

    try {
      const response = await fetch(sessionMessageUrl(props.token.trim(), selected), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(
          mode() === "full-stop"
            ? { mode: mode(), reason: bodyText }
            : { mode: mode(), text: bodyText }
        )
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      const rendered = (await response.json()) as MessageResponse;
      setOutbox((items) => {
        const existing = items.find((item) => item.messageId === rendered.message_id);
        const renderedDetail = formatUnknown(rendered.detail);
        const keepExistingStatus =
          existing !== undefined &&
          messageStatusRank(existing.status) > messageStatusRank(rendered.status);
        return [
          {
            messageId: rendered.message_id,
            sessionId: rendered.session_id,
            mode: rendered.mode,
            status: keepExistingStatus ? existing.status : rendered.status,
            text: bodyText,
            detail: keepExistingStatus ? existing.detail : renderedDetail,
            updatedAt: keepExistingStatus ? existing.updatedAt : new Date()
          },
          ...items.filter(
            (item) => item.messageId !== pendingId && item.messageId !== rendered.message_id
          )
        ].slice(0, 12);
      });
      setText("");
    } catch (caught: unknown) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setOutbox((items) =>
        items.map((item) =>
          item.messageId === pendingId
            ? { ...item, status: "delivery-failed", detail: message, updatedAt: new Date() }
            : item
        )
      );
    }
  };

  return (
    <Panel title="Message">
      <div class="grid gap-4 p-4">
        <div class="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
          <label class="grid gap-1 text-xs text-[#5b6558]">
            Session
            <select
              class="min-w-0 rounded border border-[#c8d0c0] px-3 py-2 text-sm text-[#172018]"
              value={sessionId()}
              onChange={(event) => setSessionId(event.currentTarget.value)}
            >
              <For each={props.rows}>
                {(row) => <option value={row.sessionId}>{row.sessionId}</option>}
              </For>
            </select>
          </label>
          <div class="grid gap-1 text-xs text-[#5b6558]">
            Mode
            <div class="inline-grid grid-cols-3 overflow-hidden rounded border border-[#c8d0c0] text-sm">
              <For each={(["queue", "interrupt", "full-stop"] as MessageMode[])}>
                {(item) => (
                  <button
                    class="px-3 py-2 hover:bg-[#eef2e8]"
                    classList={{
                      "bg-[#dfe8d5] font-medium": mode() === item,
                      "text-[#8f221d]": item === "full-stop"
                    }}
                    type="button"
                    onClick={() => chooseMode(item)}
                  >
                    {modeLabel(item)}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>
        <textarea
          class="min-h-28 w-full resize-y rounded border border-[#c8d0c0] px-3 py-2 text-sm text-[#172018]"
          value={text()}
          onInput={(event) => setText(event.currentTarget.value)}
        />
        <div class="flex flex-wrap items-center justify-between gap-3">
          <Show when={error().length > 0} fallback={<div />}>
            <ErrorBlock text={error()} />
          </Show>
          <button
            class="rounded bg-[#254c2b] px-4 py-2 text-sm text-white hover:bg-[#1d3d22]"
            type="button"
            onClick={send}
          >
            Send
          </button>
        </div>
        <Show when={outbox().length > 0}>
          <div class="divide-y divide-[#edf0e8] border-t border-[#edf0e8]">
            <For each={outbox()}>
              {(item) => (
                <article class="grid gap-2 py-3 text-sm md:grid-cols-[1fr_auto] md:items-center">
                  <div class="min-w-0">
                    <div class="truncate font-medium">{item.sessionId}</div>
                    <div class="truncate text-xs text-[#5b6558]">{item.text}</div>
                  </div>
                  <div class="flex items-center gap-2">
                    <StatusPill status={item.status} />
                    <div class="text-xs text-[#5b6558]">{latestTime(item.updatedAt)}</div>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Panel>
  );
}

function MaestroView(props: { events: BusEvent[] }) {
  return (
    <Panel title="Maestro">
      <EventList events={props.events} empty="No Maestro events." />
    </Panel>
  );
}

function JournalView(props: { events: BusEvent[] }) {
  return (
    <Panel title="Journal">
      <EventList events={props.events} empty="No journal events." showPayload />
    </Panel>
  );
}

function TracesView(props: { rows: TraceRow[] }) {
  return (
    <Panel title="Traces">
      <Show when={props.rows.length > 0} fallback={<EmptyState text="No traces." />}>
        <div class="divide-y divide-[#edf0e8]">
          <For each={props.rows}>
            {(trace) => (
              <article class="grid gap-1 px-4 py-3 md:grid-cols-[1fr_80px_180px] md:items-center">
                <a
                  class="min-w-0 truncate text-sm font-medium text-[#254c2b] hover:underline"
                  href={`/traces/${trace.traceId}`}
                >
                  {trace.traceId}
                </a>
                <div class="text-sm text-[#5b6558]">{trace.events} events</div>
                <div class="text-xs text-[#5b6558]">{latestTime(trace.updatedAt)}</div>
                <div class="min-w-0 truncate text-xs text-[#5b6558] md:col-span-3">
                  {trace.latestSubject}
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
    </Panel>
  );
}

function TraceReplayDetailView(props: { state: TraceReplayState }) {
  return (
    <Panel title="Trace Replay">
      <div class="p-4">
        <Show when={props.state.status === "loading"}>
          <EmptyState text="Loading trace." />
        </Show>
        <Show when={props.state.status === "error" && props.state}>
          {(state) => <ErrorBlock text={state().error} />}
        </Show>
        <Show when={props.state.status === "loaded" && props.state}>
          {(state) => {
            const replay = state().replay;
            return (
              <div class="space-y-4">
                <div class="grid gap-3 text-sm md:grid-cols-3">
                  <KeyValue label="Trace" value={replay.requested_trace_id} />
                  <KeyValue label="Chain" value={replay.chain.join(" <- ")} />
                  <KeyValue label="Entries" value={String(replay.entries.length)} />
                </div>
                <div class="divide-y divide-[#edf0e8] border-y border-[#edf0e8]">
                  <For each={replay.entries}>
                    {(entry) => (
                      <article class="grid gap-2 py-3">
                        <div class="flex flex-wrap items-center justify-between gap-2">
                          <div class="min-w-0 truncate text-sm font-medium">
                            {entry.event_type}
                          </div>
                          <div class="text-xs text-[#5b6558]">
                            {new Date(entry.timestamp).toLocaleString()}
                          </div>
                        </div>
                        <dl class="grid gap-2 text-xs text-[#5b6558] md:grid-cols-4">
                          <KeyValue label="Actor" value={entry.actor} />
                          <KeyValue label="Trace" value={entry.trace_id} />
                          <KeyValue label="Parent" value={entry.parent_trace_id ?? "-"} />
                          <KeyValue label="Seq" value={String(entry.journal_seq)} />
                        </dl>
                        <div class="min-w-0 truncate text-xs text-[#5b6558]">
                          {entry.path}:{entry.line_number}
                        </div>
                        <pre class="max-h-56 overflow-auto whitespace-pre-wrap rounded border border-[#edf0e8] bg-[#f8faf5] p-3 text-xs text-[#3c4439]">
                          {formatUnknown(entry.payload)}
                        </pre>
                      </article>
                    )}
                  </For>
                </div>
              </div>
            );
          }}
        </Show>
      </div>
    </Panel>
  );
}

function QuotasView(props: { rows: QuotaRow[] }) {
  return (
    <Panel title="Quotas">
      <Show when={props.rows.length > 0} fallback={<EmptyState text="No quota states." />}>
        <div class="max-w-full overflow-x-auto">
          <table class="w-full min-w-[680px] border-collapse text-left text-sm">
            <thead class="border-b border-[#d7ddce] text-xs uppercase text-[#5b6558]">
              <tr>
                <th class="px-4 py-2 font-medium">Window</th>
                <th class="px-4 py-2 font-medium">Status</th>
                <th class="px-4 py-2 font-medium">Remaining</th>
                <th class="px-4 py-2 font-medium">Usage</th>
                <th class="px-4 py-2 font-medium">Updated</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-[#edf0e8]">
              <For each={props.rows}>
                {(row) => (
                  <tr>
                    <td class="px-4 py-3 font-medium">{row.key}</td>
                    <td class="px-4 py-3">
                      <StatusPill status={row.status} />
                    </td>
                    <td class="px-4 py-3">{row.remaining}</td>
                    <td class="px-4 py-3">{row.usage}</td>
                    <td class="px-4 py-3 text-[#5b6558]">{latestTime(row.updatedAt)}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </Panel>
  );
}

function HealthView(props: {
  status: string;
  subject: string;
  lastConnectedAt: Date | null;
  events: BusEvent[];
}) {
  return (
    <div class="grid gap-5 md:grid-cols-2">
      <Panel title="UI Server">
        <dl class="grid gap-3 p-4 text-sm">
          <KeyValue label="Status" value={props.status} />
          <KeyValue label="Subject" value={props.subject} />
          <KeyValue label="Connected" value={latestTime(props.lastConnectedAt ?? undefined)} />
          <KeyValue label="Last event" value={latestTime(props.events[0]?.receivedAt)} />
        </dl>
      </Panel>
      <Panel title="Latest Event">
        <EventList events={props.events.slice(0, 1)} empty="No event." />
      </Panel>
    </div>
  );
}

function SettingsView(props: { tokenSaved: boolean; subject: string }) {
  return (
    <Panel title="Settings">
      <dl class="grid gap-3 p-4 text-sm">
        <KeyValue label="Token" value={props.tokenSaved ? "stored" : "empty"} />
        <KeyValue label="Subject" value={props.subject} />
        <KeyValue label="Notification subject" value="notify.human" />
      </dl>
    </Panel>
  );
}

/**
 * Three-state theme picker (system / light / dark) with sun & moon icons.
 * The popover/segmented control pattern users expect from modern dashboards;
 * a single click cycles to the next mode, and the menu lets users pick
 * `system` to auto-follow the OS pref.
 */
function ThemeToggle(props: {
  mode: ThemeMode;
  effective: "light" | "dark";
  onChange: (mode: ThemeMode) => void;
}) {
  const [open, setOpen] = createSignal(false);
  let containerEl: HTMLDivElement | undefined;

  const closeOnOutsideClick = (event: MouseEvent) => {
    if (!containerEl) return;
    if (containerEl.contains(event.target as Node)) return;
    setOpen(false);
  };
  createEffect(() => {
    if (!open()) return;
    document.addEventListener("click", closeOnOutsideClick);
    onCleanup(() => document.removeEventListener("click", closeOnOutsideClick));
  });

  const label = () =>
    props.mode === "system"
      ? `Theme: system (${props.effective})`
      : `Theme: ${props.mode}`;

  return (
    <div class="relative" ref={(el) => (containerEl = el)}>
      <button
        type="button"
        class="inline-flex h-9 w-9 items-center justify-center rounded-full border border-[#d7d7d7] bg-white text-[#171717] transition-colors hover:bg-[#eeeeef] focus:outline-none focus:ring-2 focus:ring-[#a3a3a3] focus:ring-offset-1 dark:border-[#3a3a3a] dark:bg-[#1a1a1a] dark:text-[#f5f5f5] dark:hover:bg-[#262626]"
        aria-label={label()}
        aria-haspopup="menu"
        aria-expanded={open()}
        title={label()}
        onClick={() => setOpen((v) => !v)}
      >
        <Show when={props.effective === "dark"} fallback={<SunIcon />}>
          <MoonIcon />
        </Show>
      </button>
      <Show when={open()}>
        <div
          role="menu"
          class="absolute right-0 z-20 mt-2 w-40 overflow-hidden rounded-lg border border-[#e5e5e5] bg-white text-sm shadow-lg dark:border-[#3a3a3a] dark:bg-[#1a1a1a]"
        >
          <For each={["system", "light", "dark"] as const}>
            {(option) => (
              <button
                type="button"
                role="menuitemradio"
                aria-checked={props.mode === option}
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-[#171717] hover:bg-[#f1f4ec] dark:text-[#f5f5f5] dark:hover:bg-[#262626]"
                classList={{ "font-medium": props.mode === option }}
                onClick={() => {
                  props.onChange(option);
                  setOpen(false);
                }}
              >
                <span class="inline-flex h-4 w-4 items-center justify-center">
                  {option === "light" ? (
                    <SunIcon class="h-4 w-4" />
                  ) : option === "dark" ? (
                    <MoonIcon class="h-4 w-4" />
                  ) : (
                    <SystemIcon class="h-4 w-4" />
                  )}
                </span>
                <span class="flex-1 capitalize">{option}</span>
                <Show when={props.mode === option}>
                  <span aria-hidden="true" class="text-[#5b6558]">
                    ✓
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function SunIcon(props: { class?: string } = {}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class={props.class ?? "h-4 w-4"}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
    </svg>
  );
}

function MoonIcon(props: { class?: string } = {}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class={props.class ?? "h-4 w-4"}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  );
}

function SystemIcon(props: { class?: string } = {}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class={props.class ?? "h-4 w-4"}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8M12 16v4" />
    </svg>
  );
}

function Panel(props: { title: string; children: JSX.Element }) {
  return (
    <section class="min-w-0 overflow-hidden rounded-lg border border-[#e5e5e5] bg-white shadow-[0_1px_2px_rgba(23,23,23,0.04)]">
      <div class="border-b border-[#eeeeee] px-4 py-3">
        <h2 class="text-base font-semibold">{props.title}</h2>
      </div>
      {props.children}
    </section>
  );
}

function StatTile(props: { stat: Stat }) {
  return (
    <div class="rounded-lg border border-[#e5e5e5] bg-white p-4 shadow-[0_1px_2px_rgba(23,23,23,0.04)]">
      <div class="text-xs uppercase text-[#62665e]">{props.stat.label}</div>
      <div class="mt-1 text-xl font-semibold">{props.stat.value}</div>
      <div class="mt-1 truncate text-xs text-[#62665e]">{props.stat.detail}</div>
    </div>
  );
}

function ServiceTable(props: { rows: ServiceRow[]; compact?: boolean }) {
  const rows = createMemo(() =>
    [...props.rows]
      .sort(serviceSort)
      .slice(0, props.compact === true ? 12 : undefined)
  );
  return (
    <Show when={rows().length > 0} fallback={<EmptyState text="No runtime services." />}>
      <div class="max-w-full overflow-x-auto">
        <table class="w-full min-w-[760px] border-collapse text-left text-sm">
          <thead class="border-b border-[#d7ddce] text-xs uppercase text-[#5b6558]">
            <tr>
              <th class="px-4 py-2 font-medium">Service</th>
              <th class="px-4 py-2 font-medium">Status</th>
              <th class="px-4 py-2 font-medium">Health</th>
              <th class="px-4 py-2 font-medium">PID</th>
              <th class="px-4 py-2 font-medium">Restarts</th>
              <th class="px-4 py-2 font-medium">Age</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-[#edf0e8]">
            <For each={rows()}>
              {(row) => (
                <tr>
                  <td class="max-w-72 truncate px-4 py-3 font-medium">{row.name}</td>
                  <td class="px-4 py-3">
                    <StatusPill status={row.status} />
                  </td>
                  <td class="px-4 py-3">{row.ready}</td>
                  <td class="px-4 py-3">{row.pid > 0 ? row.pid : "-"}</td>
                  <td class="px-4 py-3">{row.restarts}</td>
                  <td class="px-4 py-3 text-[#5b6558]">{row.uptime}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  );
}

function TaskTable(props: { rows: TaskRow[]; compact?: boolean }) {
  const rows = createMemo(() => props.rows.slice(0, props.compact === true ? 8 : undefined));
  return (
    <Show when={rows().length > 0} fallback={<EmptyState text="No tasks." />}>
      <Show when={props.compact === true}>
        <div class="divide-y divide-[#ecebe3]">
          <For each={rows()}>
            {(row) => (
              <article class="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
                <div class="min-w-0">
                  <a
                    class="text-sm font-medium text-[#284b35] hover:underline"
                    href={`/tasks/${encodeURIComponent(row.taskId)}`}
                  >
                    {taskDisplayName(row)}
                  </a>
                  <div class="mt-1 truncate text-xs text-[#62665e]">
                    {taskUsefulDescription(row)}
                  </div>
                  <div class="mt-2 flex flex-wrap gap-2 text-xs text-[#62665e]">
                    <span>{row.project}</span>
                    <span>{row.taskClass}</span>
                    <span>{row.priority}</span>
                    <Show when={row.prRef !== "-"}>
                      <PrLink prRef={row.prRef} />
                    </Show>
                  </div>
                </div>
                <div class="flex flex-wrap items-center gap-2 sm:justify-end">
                  <StatusPill status={row.status} />
                  <span class="text-xs text-[#62665e]">{latestTime(row.updatedAt)}</span>
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.compact !== true}>
        <div class="divide-y divide-[#edf0e8] text-sm">
          <div class="hidden grid-cols-[minmax(0,2fr)_minmax(7rem,0.75fr)_minmax(0,0.7fr)_minmax(0,0.85fr)_minmax(0,0.7fr)_minmax(0,0.9fr)_minmax(6.5rem,0.75fr)] gap-3 px-4 py-2 text-xs uppercase text-[#5b6558] lg:grid">
            <div class="font-medium">Task</div>
            <div class="font-medium">Status</div>
            <div class="font-medium">Project</div>
            <div class="font-medium">Class</div>
            <div class="font-medium">Priority</div>
            <div class="font-medium">PR</div>
            <div class="font-medium">Updated</div>
          </div>
          <For each={rows()}>
            {(row) => (
              <article class="grid min-w-0 gap-3 px-4 py-3 lg:grid-cols-[minmax(0,2fr)_minmax(7rem,0.75fr)_minmax(0,0.7fr)_minmax(0,0.85fr)_minmax(0,0.7fr)_minmax(0,0.9fr)_minmax(6.5rem,0.75fr)] lg:items-center">
                <div class="min-w-0">
                  <a
                    class="block break-words font-medium leading-5 text-[#284b35] hover:underline"
                    href={`/tasks/${encodeURIComponent(row.taskId)}`}
                  >
                    {taskDisplayName(row)}
                  </a>
                  <div class="mt-1 line-clamp-2 break-words text-xs leading-5 text-[#5b6558]">
                    {taskUsefulDescription(row)}
                  </div>
                </div>
                <div class="min-w-0">
                  <div class="mb-1 text-[0.68rem] uppercase text-[#777b72] lg:hidden">Status</div>
                  <StatusPill status={row.status} />
                </div>
                <TaskMetaCell label="Project" value={row.project} />
                <TaskMetaCell label="Class" value={row.taskClass} />
                <TaskMetaCell label="Priority" value={row.priority} />
                <TaskMetaCell label="PR" value={row.prRef} link="pr" />
                <TaskMetaCell label="Updated" value={latestTime(row.updatedAt)} muted />
              </article>
            )}
          </For>
        </div>
      </Show>
    </Show>
  );
}

function TaskMetaCell(props: { label: string; value: string; muted?: boolean; link?: "pr" }) {
  return (
    <div class={`min-w-0 ${props.muted === true ? "text-[#5b6558]" : ""}`}>
      <div class="mb-1 text-[0.68rem] uppercase text-[#777b72] lg:hidden">{props.label}</div>
      <div class="truncate" title={props.value}>
        <Show when={props.link === "pr"} fallback={props.value}>
          <PrLink prRef={props.value} />
        </Show>
      </div>
    </div>
  );
}

/**
 * Compact panel for PRs that auto-merge can't drive forward on its own —
 * either CI failed, CodeRabbit is at CHANGES_REQUESTED, or the picker's
 * continuation loop hit the cap. Shown only when there's at least one
 * stuck PR (the parent Show in DashboardView gates the whole panel).
 */
function StuckPrTable(props: { rows: PrRow[] }) {
  const rows = createMemo(() =>
    [...props.rows].sort((a, b) => b.continuationAttempt - a.continuationAttempt)
  );
  return (
    <div class="divide-y divide-[#ecebe3] text-sm">
      <For each={rows()}>
        {(row) => {
          const reason = () => {
            if (row.continuationAttempt >= CONTINUATION_ATTEMPT_CAP) {
              return `picker capped at ${row.continuationAttempt} attempts${
                row.continuationReason ? ` (${row.continuationReason})` : ""
              }`;
            }
            const ci = row.ciStatus.toLowerCase();
            if (ci === "failure" || ci === "failed" || ci === "error") {
              return `CI ${ci}`;
            }
            return row.continuationReason || "review pending";
          };
          return (
            <article class="grid gap-1 px-4 py-3">
              <div class="flex items-center justify-between gap-3">
                <PrLink prRef={row.prRef} />
                <span class="text-xs text-[#a02828]">{reason()}</span>
              </div>
              <div class="text-xs text-[#62665e]">
                task {row.taskId} · attempt {row.continuationAttempt}/
                {CONTINUATION_ATTEMPT_CAP} · CI {row.ciStatus} · {latestTime(row.updatedAt)}
              </div>
            </article>
          );
        }}
      </For>
    </div>
  );
}

function PrTable(props: { rows: PrRow[]; compact?: boolean }) {
  const rows = createMemo(() => props.rows.slice(0, props.compact === true ? 8 : undefined));
  return (
    <Show when={rows().length > 0} fallback={<EmptyState text="No pull requests." />}>
      <Show when={props.compact === true}>
        <div class="divide-y divide-[#ecebe3]">
          <For each={rows()}>
            {(row) => (
              <article class="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
                <div class="min-w-0">
                  <PrLink prRef={row.prRef} class="text-sm font-medium" />
                  <div class="mt-1 truncate text-xs text-[#62665e]">{row.title}</div>
                  <div class="mt-1 truncate text-xs text-[#62665e]">{titleFromTaskId(row.taskId)}</div>
                </div>
                <div class="flex flex-wrap items-center gap-2 sm:justify-end">
                  <StatusPill status={row.status} />
                  <StatusPill status={row.ciStatus} />
                  <span class="text-xs text-[#62665e]">{latestTime(row.updatedAt)}</span>
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.compact !== true}>
      <div class="max-w-full overflow-x-auto">
        <table class="w-full min-w-[760px] border-collapse text-left text-sm">
          <thead class="border-b border-[#d7ddce] text-xs uppercase text-[#5b6558]">
            <tr>
              <th class="px-4 py-2 font-medium">PR</th>
              <th class="px-4 py-2 font-medium">Status</th>
              <th class="px-4 py-2 font-medium">CI</th>
              <th class="px-4 py-2 font-medium">Review</th>
              <th class="px-4 py-2 font-medium">Task</th>
              <th class="px-4 py-2 font-medium">Updated</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-[#edf0e8]">
            <For each={rows()}>
              {(row) => (
                <tr>
                  <td class="min-w-72 px-4 py-3">
                    <PrLink prRef={row.prRef} class="font-medium" />
                    <div class="mt-1 max-w-xl truncate text-xs text-[#5b6558]">{row.title}</div>
                  </td>
                  <td class="px-4 py-3">
                    <StatusPill status={row.status} />
                  </td>
                  <td class="px-4 py-3">
                    <StatusPill status={row.ciStatus} />
                  </td>
                  <td class="max-w-48 truncate px-4 py-3">{row.review}</td>
                  <td class="max-w-56 truncate px-4 py-3">{titleFromTaskId(row.taskId)}</td>
                  <td class="px-4 py-3 text-[#5b6558]">{latestTime(row.updatedAt)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      </Show>
    </Show>
  );
}

function EventList(props: { events: BusEvent[]; empty: string; showPayload?: boolean }) {
  return (
    <div class="divide-y divide-[#edf0e8]">
      <Show when={props.events.length > 0} fallback={<EmptyState text={props.empty} />}>
        <For each={props.events}>
          {(event) => {
            const update = eventUpdate(event);
            return (
              <article class="grid gap-2 px-4 py-3">
                <div class="flex gap-3">
                  <StatusSymbol status={update.status} />
                  <div class="min-w-0 flex-1">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                      <div class="min-w-0 truncate text-sm font-medium">{update.title}</div>
                      <div class="text-xs text-[#5b6558]">{latestTime(update.updatedAt)}</div>
                    </div>
                    <div class="mt-1 text-sm text-[#3c4439]">{update.detail}</div>
                    <Show when={props.showPayload === true}>
                      <details class="mt-2 text-xs text-[#5b6558]">
                        <summary class="cursor-pointer">Technical payload</summary>
                        <pre class="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded border border-[#edf0e8] bg-[#f8faf5] p-3 text-xs text-[#3c4439]">
                          {formatPayload(event)}
                        </pre>
                      </details>
                    </Show>
                  </div>
                </div>
              </article>
            );
          }}
        </For>
      </Show>
    </div>
  );
}

function EmptyState(props: { text: string }) {
  return <div class="px-4 py-8 text-sm text-[#5b6558]">{props.text}</div>;
}

function ErrorBlock(props: { text: string }) {
  return (
    <div class="rounded border border-[#b2332e] bg-[#fff0ee] px-4 py-3 text-sm text-[#8f221d]">
      {props.text}
    </div>
  );
}

function StatusPill(props: { status: string }) {
  const meta = createMemo(() => statusMeta(props.status));
  return (
    <span
      class={`inline-flex max-w-full items-center gap-1 whitespace-nowrap rounded border px-2 py-0.5 text-xs uppercase ${meta().classes}`}
      title={meta().meaning}
    >
      <span aria-hidden="true">{meta().symbol}</span>
      {meta().label}
    </span>
  );
}

function StatusSymbol(props: { status: string; compact?: boolean }) {
  const meta = createMemo(() => statusMeta(props.status));
  return (
    <span
      class={`inline-flex shrink-0 items-center justify-center rounded-md border font-semibold ${props.compact === true ? "h-6 w-6 text-xs" : "h-9 w-9 text-base"} ${meta().classes}`}
      title={meta().meaning}
      aria-label={meta().label}
    >
      {meta().symbol}
    </span>
  );
}

function statusMeta(status: string) {
  const normalized = normalizeStatus(status);
  const label = statusLabel(normalized);
  const success = "border-[#8aa08a] bg-[#f4f7f0] text-[#3f6a42]";
  const active = "border-[#6f7f9c] bg-[#f1f4fb] text-[#375785]";
  const waiting = "border-[#b46b14] bg-[#fff6e8] text-[#84500f]";
  const danger = "border-[#b2332e] bg-[#fff0ee] text-[#8f221d]";
  const neutral = "border-[#c9c8c0] bg-[#f8f8f4] text-[#575b53]";

  if (
    [
      "available",
      "delivered",
      "kill-confirmed",
      "ready",
      "steady",
      "success",
      "merged"
    ].includes(normalized)
  ) {
    return { label, symbol: "✓", classes: success, meaning: "complete or healthy" };
  }
  if (
    [
      "running",
      "sending",
      "queued",
      "in-progress",
      "in-review",
      "open",
      "pr-open",
      "resuming",
      "interrupt-requested",
      "interrupt-accepted"
    ].includes(normalized)
  ) {
    return { label, symbol: "●", classes: active, meaning: "currently moving" };
  }
  if (
    [
      "low",
      "kill-requested",
      "draft",
      "review",
      "pending",
      "disabled",
      "completed",
      "picker-completed",
      "exited",
      "closed"
    ].includes(normalized)
  ) {
    return { label, symbol: "◐", classes: waiting, meaning: "waiting for the next handoff" };
  }
  if (
    [
      "exhausted",
      "failed",
      "failure",
      "error",
      "abandoned",
      "killed",
      "delivery-failed"
    ].includes(normalized)
  ) {
    return { label, symbol: "!", classes: danger, meaning: "needs attention" };
  }
  if (normalized === "backlog" || normalized === "unknown") {
    return { label, symbol: "○", classes: neutral, meaning: "known but not currently active" };
  }
  return { label, symbol: "○", classes: neutral, meaning: "status has no specific mapping" };
}

function statusLabel(status: string) {
  const normalized = normalizeStatus(status);
  const labels: Record<string, string> = {
    abandoned: "abandoned",
    available: "available",
    backlog: "backlog",
    closed: "closed",
    completed: "completed",
    delivered: "delivered",
    disabled: "disabled",
    draft: "draft PR",
    error: "error",
    exhausted: "exhausted",
    exited: "exited",
    failed: "failed",
    failure: "failed",
    "in-progress": "in progress",
    "in-review": "in review",
    killed: "killed",
    low: "low",
    merged: "merged",
    open: "open",
    pending: "pending",
    "picker-completed": "ready for PR",
    "pr-open": "PR open",
    queued: "queued",
    ready: "ready",
    review: "review",
    running: "running",
    sending: "sending",
    steady: "steady",
    success: "success",
    unknown: "unknown"
  };
  return labels[normalized] ?? normalized.replaceAll("-", " ");
}

function normalizeStatus(status: unknown) {
  return typeof status === "string"
    ? status.trim().toLowerCase().replaceAll("_", "-") || "unknown"
    : "unknown";
}

function KeyValue(props: { label: string; value: string }) {
  return (
    <div class="grid gap-1 border-b border-[#edf0e8] pb-3 last:border-b-0 last:pb-0">
      <dt class="text-xs uppercase text-[#5b6558]">{props.label}</dt>
      <dd class="min-w-0 truncate font-medium">{props.value}</dd>
    </div>
  );
}

function KeyValueCustom(props: { label: string; children: JSX.Element }) {
  return (
    <div class="grid gap-1 border-b border-[#edf0e8] pb-3 last:border-b-0 last:pb-0">
      <dt class="text-xs uppercase text-[#5b6558]">{props.label}</dt>
      <dd class="min-w-0 truncate font-medium">{props.children}</dd>
    </div>
  );
}

function PrLink(props: { prRef: string; class?: string }) {
  const url = prUrl(props.prRef);
  if (!url) {
    return <span class={props.class}>{props.prRef}</span>;
  }
  return (
    <a
      class={`text-[#254c2b] hover:underline ${props.class ?? ""}`}
      href={url}
      target="_blank"
      rel="noreferrer"
    >
      {prLabel(props.prRef)}
    </a>
  );
}

function NotificationDrawer(props: {
  open: boolean;
  notifications: HumanNotification[];
  onClose: () => void;
  onClear: () => void;
}) {
  return (
    <Show when={props.open}>
      <div class="fixed inset-0 z-20 bg-black/20" onClick={props.onClose} />
      <aside class="fixed right-0 top-0 z-30 flex h-screen w-full max-w-md flex-col border-l border-[#c8d0c0] bg-white shadow-xl">
        <div class="flex items-center justify-between border-b border-[#d7ddce] px-4 py-3">
          <h2 class="text-base font-semibold">Notifications</h2>
          <div class="flex items-center gap-2">
            <button
              class="rounded border border-[#c8d0c0] px-3 py-1 text-sm hover:bg-[#eef2e8]"
              type="button"
              onClick={props.onClear}
            >
              Clear
            </button>
            <button
              class="rounded border border-[#c8d0c0] px-3 py-1 text-sm hover:bg-[#eef2e8]"
              type="button"
              onClick={props.onClose}
            >
              Close
            </button>
          </div>
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto">
          <Show
            when={props.notifications.length > 0}
            fallback={<EmptyState text="No notifications." />}
          >
            <For each={props.notifications}>
              {(notification) => (
                <article class="border-b border-[#edf0e8] px-4 py-3">
                  <div class="mb-2 flex items-start justify-between gap-3">
                    <div class="min-w-0 text-sm font-medium">{notification.summary}</div>
                    <StatusPill status={notification.urgency} />
                  </div>
                  <div class="text-xs text-[#5b6558]">{notification.receivedAt}</div>
                  <Show when={notification.detail.length > 0}>
                    <pre class="mt-3 max-h-56 overflow-auto whitespace-pre-wrap rounded border border-[#edf0e8] bg-[#f8faf5] p-3 text-xs text-[#3c4439]">
                      {notification.detail}
                    </pre>
                  </Show>
                </article>
              )}
            </For>
          </Show>
        </div>
      </aside>
    </Show>
  );
}

function routeView(path: string, data: RouteData) {
  if (path.startsWith("/tasks/")) {
    return (
      <TaskDetailView
        taskId={decodeURIComponent(path.slice("/tasks/".length))}
        rows={data.taskRows}
        events={data.events}
        prs={data.prRows}
        pickers={data.pickerRows}
        token={data.token}
      />
    );
  }
  if (path === "/tasks") {
    return (
      <TasksView
        rows={data.taskRows}
        token={data.token}
        createTaskState={data.createTaskState}
        onCreateTaskState={data.onCreateTaskState}
      />
    );
  }
  if (path === "/prs") {
    return <PullRequestsView rows={data.prRows} />;
  }
  if (path === "/pickers") {
    return <PickersView rows={data.pickerRows} events={data.events} token={data.token} />;
  }
  if (path === "/maestro") {
    return <MaestroView events={data.maestroEvents} />;
  }
  if (path === "/journal") {
    return <JournalView events={data.events} />;
  }
  if (path.startsWith("/traces/")) {
    return <TraceReplayDetailView state={data.traceReplay} />;
  }
  if (path === "/traces") {
    return <TracesView rows={data.traceRows} />;
  }
  if (path === "/quotas") {
    return <QuotasView rows={data.quotaRows} />;
  }
  if (path === "/health") {
    return (
      <HealthView
        status={data.status}
        subject={data.subject}
        lastConnectedAt={data.lastConnectedAt}
        events={data.events}
      />
    );
  }
  if (path === "/settings") {
    return (
      <SettingsView
        tokenSaved={localStorage.getItem("jam.ui.token") !== null}
        subject={data.subject}
      />
    );
  }
  return (
    <DashboardView
      status={data.status}
      subject={data.subject}
      lastConnectedAt={data.lastConnectedAt}
      events={data.events}
      notifications={data.notifications}
      services={data.services}
      taskRows={data.taskRows}
      pickerRows={data.pickerRows}
      prRows={data.prRows}
      traceRows={data.traceRows}
      quotaRows={data.quotaRows}
      quotaError={data.quotaError}
      quotaRefreshedAt={data.quotaRefreshedAt}
      token={data.token}
      createTaskState={data.createTaskState}
      onCreateTaskState={data.onCreateTaskState}
      deployTargets={data.deployTargets}
    />
  );
}

function wsUrl(token: string, subject: string) {
  const url = new URL("/ws", window.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("token", token);
  url.searchParams.set("subject", subject);
  return url;
}

function recentEventsUrl(token: string, subject: string) {
  const url = new URL("/api/events/recent", window.location.href);
  url.searchParams.set("token", token);
  url.searchParams.set("subject", subject);
  url.searchParams.set("limit", "500");
  return url;
}

function runtimeServicesUrl(token: string) {
  const url = new URL("/api/runtime/services", window.location.href);
  url.searchParams.set("token", token);
  return url;
}

function deployTargetsUrl(token: string) {
  const url = new URL("/api/deploy", window.location.href);
  url.searchParams.set("token", token);
  return url;
}

function tasksUrl(token: string) {
  const url = new URL("/api/tasks", window.location.href);
  url.searchParams.set("token", token);
  return url;
}

function quotaUrl(token: string) {
  const url = new URL("/api/quota", window.location.href);
  url.searchParams.set("token", token);
  return url;
}

function traceReplayUrl(token: string, traceId: string) {
  const url = new URL(`/api/trace/${encodeURIComponent(traceId)}`, window.location.href);
  url.searchParams.set("token", token);
  url.searchParams.set("max_depth", "5");
  return url;
}

function sessionMessageUrl(token: string, sessionId: string) {
  const url = new URL(
    `/api/sessions/${encodeURIComponent(sessionId)}/messages`,
    window.location.href
  );
  url.searchParams.set("token", token);
  return url;
}

function taskResumeUrl(token: string, taskId: string) {
  const url = new URL(`/api/tasks/${encodeURIComponent(taskId)}/resume`, window.location.href);
  url.searchParams.set("token", token);
  return url;
}

function sessionOutputUrl(token: string, sessionId: string) {
  const url = new URL(
    `/api/sessions/${encodeURIComponent(sessionId)}/output`,
    window.location.href
  );
  url.searchParams.set("token", token);
  url.searchParams.set("limit", "300");
  return url;
}

function parseBusEvent(raw: unknown): BusEvent {
  const parsed = JSON.parse(String(raw)) as WireBusEvent;
  return parseWireBusEvent(parsed, "live");
}

function parseWireBusEvent(parsed: WireBusEvent, source: BusEvent["source"]): BusEvent {
  const payload = typeof parsed.payload === "string" ? parsed.payload : "{}";
  const envelope = parseObject(payload);
  return {
    id: eventKey(
      typeof parsed.subject === "string" ? parsed.subject : "unknown",
      payload,
      envelope
    ),
    subject: typeof parsed.subject === "string" ? parsed.subject : "unknown",
    payload,
    receivedAt: eventTimestamp(envelope),
    envelope,
    source
  };
}

function parseSessionOutputRecord(raw: unknown): SessionOutputRecord {
  return outputRecordFromPayload(recordFromUnknown(raw));
}

function outputRecordFromEvent(event: BusEvent): SessionOutputRecord {
  return outputRecordFromPayload(event.envelope);
}

function outputRecordFromPayload(payload: Record<string, unknown>): SessionOutputRecord {
  const sessionId = stringField(payload, "session_id") ?? "-";
  const taskId = stringField(payload, "task_id") ?? "-";
  const traceId = stringField(payload, "trace_id") ?? "-";
  const stream = normalizeOutputStream(stringField(payload, "stream"));
  const sequence = numberField(payload, "sequence") ?? 0;
  const ts = parseDate(stringField(payload, "ts"));
  const line = stringField(payload, "line") ?? "";
  return {
    id: `${sessionId}|${sequence}|${stream}|${ts.toISOString()}|${line.slice(0, 24)}`,
    sessionId,
    taskId,
    traceId,
    stream,
    line,
    ts,
    sequence,
    truncated: boolField(payload, "truncated") === true
  };
}

function normalizeOutputStream(stream: string | undefined): SessionOutputRecord["stream"] {
  if (stream === "stdout" || stream === "stderr") {
    return stream;
  }
  return "output";
}

function mergeOutputRecords(incoming: SessionOutputRecord[], existing: SessionOutputRecord[]) {
  const byId = new Map<string, SessionOutputRecord>();
  for (const record of [...incoming, ...existing]) {
    byId.set(record.id, record);
  }
  return [...byId.values()]
    .sort((left, right) => {
      const byTime = left.ts.getTime() - right.ts.getTime();
      return byTime === 0 ? left.sequence - right.sequence : byTime;
    })
    .slice(-300);
}

function parseServiceRow(raw: unknown): ServiceRow {
  const item = recordFromUnknown(raw);
  const status = stringField(item, "status") ?? "unknown";
  const ready = stringField(item, "is_ready") ?? stringField(item, "health") ?? "-";
  const running = boolField(item, "IsRunning") ?? status.toLowerCase() === "running";
  return {
    name: stringField(item, "name") ?? "unknown",
    status,
    ready,
    pid: numberField(item, "pid") ?? 0,
    restarts: numberField(item, "restarts") ?? 0,
    uptime: stringField(item, "system_time") ?? "-",
    running
  };
}

function parseDeployTargetRow(raw: unknown): DeployTargetRow {
  const item = recordFromUnknown(raw);
  return {
    shortName: stringField(item, "short_name") ?? "unknown",
    crateName: stringField(item, "crate_name") ?? "",
    binaryName: stringField(item, "binary_name") ?? "",
    strategy: stringField(item, "strategy") ?? "unknown"
  };
}

function mergeEvents(incoming: BusEvent[], existing: BusEvent[]) {
  const byId = new Map<string, BusEvent>();
  for (const event of [...incoming, ...existing]) {
    byId.set(event.id, event);
  }
  return [...byId.values()]
    .sort((left, right) => right.receivedAt.getTime() - left.receivedAt.getTime())
    .slice(0, 500);
}

function eventKey(subject: string, payload: string, envelope: Record<string, unknown>) {
  const traceId = stringField(envelope, "trace_id");
  const eventType = stringField(envelope, "event_type");
  const timestamp = stringField(envelope, "timestamp");
  const journalSeq = numberField(envelope, "journal_seq");
  if (traceId && eventType && timestamp) {
    return `${subject}|${eventType}|${traceId}|${timestamp}|${journalSeq ?? "n"}`;
  }
  return `${subject}|${payload}`;
}

function eventTimestamp(envelope: Record<string, unknown>) {
  const timestamp = stringField(envelope, "timestamp");
  if (timestamp) {
    const parsed = new Date(timestamp);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date();
}

function mergeTaskRows(snapshotRows: TaskRow[], eventRows: TaskRow[], liveEventRows: TaskRow[]) {
  const rows = new Map<string, TaskRow>();
  for (const row of snapshotRows) {
    rows.set(row.taskId, row);
  }
  for (const row of eventRows) {
    const existing = rows.get(row.taskId);
    if (existing) {
      rows.set(row.taskId, mergeTaskRow(existing, row));
    }
  }
  for (const row of liveEventRows) {
    const existing = rows.get(row.taskId);
    rows.set(row.taskId, mergeTaskRow(existing, row));
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function mergeTaskRow(existing: TaskRow | undefined, incoming: TaskRow): TaskRow {
  if (!existing) {
    return incoming;
  }
  const incomingNewer = incoming.updatedAt.getTime() >= existing.updatedAt.getTime();
  return {
    taskId: incoming.taskId,
    description: incoming.description || existing.description,
    project: usefulField(incoming.project, existing.project),
    taskClass: usefulField(incoming.taskClass, existing.taskClass),
    priority: usefulField(incoming.priority, existing.priority),
    status: incoming.status === "unknown" && !incomingNewer ? existing.status : incoming.status,
    requestedBy: usefulField(incoming.requestedBy, existing.requestedBy),
    prRef: usefulField(incoming.prRef, existing.prRef),
    sessionId: usefulField(incoming.sessionId, existing.sessionId),
    harness: usefulField(incoming.harness, existing.harness),
    outcome: usefulField(incoming.outcome, existing.outcome),
    traceId: usefulField(incoming.traceId, existing.traceId),
    updatedAt: incomingNewer ? incoming.updatedAt : existing.updatedAt
  };
}

function usefulField(incoming: string, existing: string) {
  return incoming && incoming !== "-" ? incoming : existing;
}

function taskRowFromGraph(raw: TaskGraphRow): TaskRow {
  const taskId = raw.task_id ?? raw.taskId ?? "unknown";
  return {
    taskId,
    description: raw.description ?? taskId,
    project: raw.project ?? "-",
    taskClass: raw.task_class ?? raw.taskClass ?? "-",
    priority: raw.priority ?? "-",
    status: raw.status ?? "unknown",
    requestedBy: raw.requested_by ?? raw.requestedBy ?? "-",
    prRef: raw.pr_ref ?? raw.prRef ?? "-",
    sessionId: raw.session_id ?? raw.sessionId ?? "-",
    harness: raw.harness ?? "-",
    outcome: raw.outcome ?? "-",
    traceId: raw.trace_id ?? raw.traceId ?? "-",
    updatedAt: parseDate(raw.updated_at ?? raw.updatedAt)
  };
}

function taskRowsFromEvents(events: BusEvent[]) {
  const rows = new Map<string, TaskRow>();
  for (const event of [...events].reverse()) {
    const eventType = stringField(event.envelope, "event_type") ?? "";
    const payload = objectField(event.envelope, "payload");
    const taskId = stringField(payload, "task_id");
    if (!taskId || !isTaskEvent(eventType)) {
      continue;
    }

    const existing = rows.get(taskId) ?? emptyTaskRow(taskId, event.receivedAt);
    const next: TaskRow = {
      ...existing,
      description:
        stringField(payload, "description") ?? stringField(payload, "title") ?? existing.description,
      project: stringField(payload, "project") ?? existing.project,
      taskClass: stringField(payload, "task_class") ?? existing.taskClass,
      priority: stringField(payload, "priority") ?? existing.priority,
      requestedBy: stringField(payload, "requested_by") ?? existing.requestedBy,
      prRef: prRefFromPayload(payload) ?? existing.prRef,
      sessionId: stringField(payload, "session_id") ?? existing.sessionId,
      harness: stringField(payload, "harness") ?? existing.harness,
      outcome: stringField(payload, "outcome") ?? existing.outcome,
      traceId:
        stringField(payload, "picker_trace_id") ??
        stringField(payload, "maestro_trace_id") ??
        stringField(event.envelope, "trace_id") ??
        existing.traceId,
      status: taskStatusFromEvent(eventType, payload, existing.status),
      updatedAt: event.receivedAt
    };
    rows.set(taskId, next);
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function mergeQuotaRows(snapshotRows: QuotaRow[], eventRows: QuotaRow[]) {
  const rows = new Map<string, QuotaRow>();
  for (const row of snapshotRows) {
    rows.set(row.key, row);
  }
  for (const row of eventRows) {
    const existing = rows.get(row.key);
    if (!existing || row.updatedAt.getTime() >= existing.updatedAt.getTime()) {
      rows.set(row.key, row);
    }
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function quotaRowsFromSnapshot(snapshot: QuotaSnapshot | null) {
  if (!snapshot) {
    return [];
  }
  const windows = recordFromUnknown(snapshot.windows);
  const fetchedAt = parseDate(snapshot.fetched_at ?? snapshot.fetchedAt);
  return Object.entries(windows)
    .map(([key, value]) => {
      const row = recordFromUnknown(value);
      const usage = objectField(row, "usage");
      const observedAt = parseDate(stringField(row, "observed_at"));
      return {
        key,
        status: stringField(row, "status") ?? "unknown",
        remaining: fractionField(row, "remaining") ?? "-",
        usage: usageSummary(row, usage),
        detail: stringField(row, "detail") ?? "-",
        source: stringField(row, "source") ?? snapshot.source ?? "tool.observe.query-quota",
        updatedAt: observedAt.getTime() > 0 ? observedAt : fetchedAt
      };
    })
    .sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function prRowsFromEvents(events: BusEvent[]) {
  const rows = new Map<string, PrRow>();
  // Build the continuation state map once per render rather than scanning
  // events again inside the loop below — events list can be long.
  const continuationByTask = continuationStateByTask(events);
  for (const event of [...events].reverse()) {
    const eventType = stringField(event.envelope, "event_type") ?? "";
    if (!eventType.startsWith("pr.")) {
      continue;
    }
    const payload = objectField(event.envelope, "payload");
    const prRef = prRefFromPayload(payload);
    if (!prRef) {
      continue;
    }
    const existing = rows.get(prRef) ?? emptyPrRow(prRef, event.receivedAt);
    const next: PrRow = {
      ...existing,
      taskId: stringField(payload, "task_id") ?? existing.taskId,
      title: stringField(payload, "title") ?? stringField(payload, "summary") ?? existing.title,
      status: prStatusFromEvent(eventType, payload, existing.status),
      ciStatus:
        eventType === "pr.ci.status-changed"
          ? ciStatusFromPayload(payload, existing.ciStatus)
          : existing.ciStatus,
      review:
        eventType === "pr.review-received"
          ? reviewSummaryFromPayload(payload)
          : existing.review,
      updatedAt: event.receivedAt,
      continuationAttempt: existing.continuationAttempt,
      continuationReason: existing.continuationReason
    };
    rows.set(prRef, next);
  }
  // Second pass: cross-reference task_id -> continuation telemetry so the
  // dashboard's stuck-PR panel has the data it needs without each row
  // re-scanning events.
  for (const row of rows.values()) {
    if (row.taskId === "-" || row.taskId === "") continue;
    const cont = continuationByTask.get(row.taskId);
    if (cont) {
      row.continuationAttempt = cont.attempt;
      row.continuationReason = cont.reason;
    }
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function pickerRowsFromEvents(events: BusEvent[]) {
  const rows = new Map<string, PickerRow>();
  for (const event of [...events].reverse()) {
    const eventType = stringField(event.envelope, "event_type");
    const payload = objectField(event.envelope, "payload");
    const sessionId = stringField(payload, "session_id");
    if (!sessionId || !eventType?.startsWith("picker.")) {
      continue;
    }
    const existing = rows.get(sessionId);
    rows.set(sessionId, {
      sessionId,
      taskId: stringField(payload, "task_id") ?? existing?.taskId ?? "-",
      harness: stringField(payload, "harness") ?? existing?.harness ?? "-",
      status: pickerStatus(eventType, existing?.status ?? "unknown"),
      worktreePath: stringField(payload, "worktree_path") ?? existing?.worktreePath ?? "-",
      lastEvent: eventType,
      updatedAt: event.receivedAt
    });
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function traceRowsFromEvents(events: BusEvent[]) {
  const rows = new Map<string, TraceRow>();
  for (const event of events) {
    const traceId = stringField(event.envelope, "trace_id");
    if (!traceId) {
      continue;
    }
    const existing = rows.get(traceId);
    rows.set(traceId, {
      traceId,
      events: (existing?.events ?? 0) + 1,
      latestSubject: existing?.latestSubject ?? event.subject,
      updatedAt: existing?.updatedAt ?? event.receivedAt
    });
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function quotaRowsFromEvents(events: BusEvent[]) {
  const rows = new Map<string, QuotaRow>();
  for (const event of [...events].reverse()) {
    const eventType = stringField(event.envelope, "event_type");
    if (!eventType?.startsWith("quota.")) {
      continue;
    }
    const payload = objectField(event.envelope, "payload");
    const harness = stringField(payload, "harness") ?? "unknown";
    const windowKind = stringField(payload, "window_kind") ?? "unknown";
    const key = `${harness}/${windowKind}`;
    const usage = objectField(payload, "usage");
    rows.set(key, {
      key,
      status: quotaStatus(eventType, payload),
      remaining: fractionField(payload, "remaining") ?? "-",
      usage: usageSummary(payload, usage),
      detail: stringField(payload, "detail") ?? "-",
      source: eventType,
      updatedAt: event.receivedAt
    });
  }
  return [...rows.values()].sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function emptyTaskRow(taskId: string, updatedAt: Date): TaskRow {
  return {
    taskId,
    description: "",
    project: "-",
    taskClass: "-",
    priority: "-",
    status: "unknown",
    requestedBy: "-",
    prRef: "-",
    sessionId: "-",
    harness: "-",
    outcome: "-",
    traceId: "-",
    updatedAt
  };
}

function emptyPrRow(prRef: string, updatedAt: Date): PrRow {
  return {
    prRef,
    taskId: "-",
    title: prRef,
    status: "unknown",
    ciStatus: "unknown",
    review: "-",
    updatedAt,
    continuationAttempt: 0,
    continuationReason: ""
  };
}

/// Per-task continuation telemetry derived from `picker.continuation-needed`
/// events. Indexed by task_id (not PR ref) since continuations may fire
/// before a PR exists.
function continuationStateByTask(events: BusEvent[]) {
  const state = new Map<string, { attempt: number; reason: string }>();
  for (const event of [...events].reverse()) {
    const eventType = stringField(event.envelope, "event_type");
    if (eventType !== "picker.continuation-needed") {
      continue;
    }
    const payload = objectField(event.envelope, "payload");
    const taskId = stringField(payload, "task_id");
    if (!taskId) continue;
    const attempt = numberField(payload, "attempt") ?? 0;
    const reason = stringField(payload, "reason") ?? "";
    const cur = state.get(taskId);
    // Keep the highest-attempt entry — events are streamed reverse-chrono so
    // the first one we see for a task is the most recent.
    if (!cur || attempt > cur.attempt) {
      state.set(taskId, { attempt, reason });
    }
  }
  return state;
}

/// Per-PR continuation cap — matches `jam-task-lifecycle::post_picker`'s
/// `CONTINUATION_ATTEMPT_CAP`. Tasks at this count or above are stuck.
const CONTINUATION_ATTEMPT_CAP = 5;

/// True when this PR is open AND something is preventing auto-merge:
///   - picker hit the continuation cap
///   - CI is in a failure state
///   - CodeRabbit posted CHANGES_REQUESTED that nobody has addressed yet
function prIsStuck(row: PrRow): boolean {
  if (!isOpenPrStatus(row.status)) return false;
  if (row.continuationAttempt >= CONTINUATION_ATTEMPT_CAP) return true;
  const ci = row.ciStatus.toLowerCase();
  if (ci === "failure" || ci === "failed" || ci === "error") return true;
  return false;
}

function isTaskEvent(eventType: string) {
  return (
    eventType === "task.requested" ||
    eventType === "task.failed" ||
    eventType === "task.abandoned" ||
    eventType.startsWith("picker.") ||
    eventType.startsWith("pr.") ||
    eventType === "tempyr.task-updated"
  );
}

function taskStatusFromEvent(
  eventType: string,
  payload: Record<string, unknown>,
  existing: string
) {
  if (eventType === "task.requested") {
    return "backlog";
  }
  if (eventType === "task.failed") {
    return "failed";
  }
  if (eventType === "picker.spawned") {
    return "running";
  }
  if (eventType === "picker.exited") {
    if (isFinalStatus(existing) || existing === "in-review" || existing === "draft") {
      return existing;
    }
    return numberField(payload, "exit_code") === 0 ? "picker-completed" : "failed";
  }
  if (eventType === "pr.opened") {
    return boolField(payload, "draft") === true ? "draft" : "in-review";
  }
  if (eventType === "pr.status-changed") {
    return prStatusFromEvent(eventType, payload, existing);
  }
  if (eventType === "pr.review-received") {
    return isFinalStatus(existing) ? existing : "review";
  }
  if (eventType === "pr.merged") {
    return "merged";
  }
  if (eventType === "task.abandoned") {
    return "abandoned";
  }
  if (eventType === "tempyr.task-updated") {
    return stringField(payload, "status") ?? existing;
  }
  return existing;
}

function prStatusFromEvent(
  eventType: string,
  payload: Record<string, unknown>,
  existing: string
) {
  if (eventType === "pr.opened") {
    return boolField(payload, "draft") === true ? "draft" : "open";
  }
  if (eventType === "pr.merged") {
    return "merged";
  }
  if (eventType === "pr.review-received") {
    return isFinalStatus(existing) ? existing : "review";
  }
  const merged = boolField(payload, "merged");
  if (merged === true) {
    return "merged";
  }
  const state = stringField(payload, "state") ?? stringField(payload, "status");
  if (state === "open" && boolField(payload, "draft") === true) {
    return "draft";
  }
  return state ?? existing;
}

function ciStatusFromPayload(payload: Record<string, unknown>, existing: string) {
  return (
    stringField(payload, "ci_status") ??
    stringField(payload, "status") ??
    stringField(payload, "conclusion") ??
    existing
  );
}

function reviewSummaryFromPayload(payload: Record<string, unknown>) {
  return (
    stringField(payload, "summary") ??
    stringField(payload, "review_state") ??
    stringField(payload, "state") ??
    stringField(payload, "reviewer") ??
    "received"
  );
}

function prRefFromPayload(payload: Record<string, unknown>) {
  return (
    stringField(payload, "pr_ref") ??
    stringField(payload, "pull_request") ??
    stringField(payload, "pr_url") ??
    stringField(payload, "url")
  );
}

function prUrl(prRef: string) {
  if (!prRef || prRef === "-") {
    return undefined;
  }
  if (prRef.startsWith("https://github.com/") && prRef.includes("/pull/")) {
    return prRef;
  }
  const match = prRef.match(/^([^/\s#]+)\/([^/\s#]+)#(\d+)$/);
  if (!match) {
    return undefined;
  }
  return `https://github.com/${match[1]}/${match[2]}/pull/${match[3]}`;
}

function prLabel(prRef: string) {
  if (!prRef.startsWith("https://github.com/")) {
    return prRef;
  }
  const match = prRef.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)/);
  return match ? `${match[1]}/${match[2]}#${match[3]}` : prRef;
}

function isFinalStatus(status: string) {
  const normalized = normalizeStatus(status);
  return (
    normalized === "merged" ||
    normalized === "abandoned" ||
    normalized === "closed" ||
    normalized === "completed" ||
    normalized === "picker-completed" ||
    normalized === "failed"
  );
}

function isOpenPrStatus(status: string) {
  const normalized = normalizeStatus(status);
  return normalized !== "merged" && normalized !== "closed" && normalized !== "abandoned";
}

function isActiveTaskStatus(status: string) {
  const normalized = normalizeStatus(status);
  return (
    normalized === "queued" ||
    normalized === "running" ||
    normalized === "in-progress" ||
    normalized === "picker-completed" ||
    normalized === "draft" ||
    normalized === "in-review" ||
    normalized === "review" ||
    normalized === "open" ||
    normalized === "pr-open"
  );
}

function isInFlightTaskStatus(status: string) {
  const normalized = normalizeStatus(status);
  return (
    normalized === "queued" ||
    normalized === "running" ||
    normalized === "in-progress" ||
    normalized === "draft" ||
    normalized === "in-review" ||
    normalized === "review" ||
    normalized === "open" ||
    normalized === "pr-open"
  );
}

function shouldResumePicker(taskStatus: string, pickerStatus: string | undefined) {
  const picker = pickerStatus ? normalizeStatus(pickerStatus) : "";
  if (picker === "running") {
    return false;
  }
  const task = normalizeStatus(taskStatus);
  return !["queued", "running", "in-progress", "backlog", "unknown"].includes(task);
}

function countByStatus(rows: TaskRow[]) {
  const counts = new Map<string, number>();
  for (const row of rows) {
    counts.set(row.status, (counts.get(row.status) ?? 0) + 1);
  }
  return counts;
}

function importantTasks(rows: TaskRow[]) {
  const rank = (status: string) => {
    if (isActiveTaskStatus(status)) {
      return 0;
    }
    if (status === "failed" || status === "abandoned") {
      return 1;
    }
    if (status === "backlog") {
      return 2;
    }
    if (status === "merged") {
      return 3;
    }
    return 4;
  };
  return [...rows].sort((left, right) => {
    const rankDiff = rank(left.status) - rank(right.status);
    if (rankDiff !== 0) {
      return rankDiff;
    }
    return right.updatedAt.getTime() - left.updatedAt.getTime();
  });
}

function quotaHeadline(rows: QuotaRow[], error: string) {
  if (error) {
    return "error";
  }
  if (rows.length === 0) {
    return "unknown";
  }
  if (rows.some((row) => row.status === "exhausted")) {
    return "exhausted";
  }
  if (rows.some((row) => row.status === "low")) {
    return "low";
  }
  if (rows.some((row) => row.status === "available")) {
    return "available";
  }
  return rows[0].status;
}

function remainingPercent(value: string) {
  const numeric = Number(value.replace("%", ""));
  if (!Number.isFinite(numeric)) {
    return 0;
  }
  return Math.max(0, Math.min(100, numeric));
}

function eventTouchesTask(event: BusEvent, taskId: string) {
  const payload = objectField(event.envelope, "payload");
  return stringField(payload, "task_id") === taskId;
}

function taskDisplayName(task: TaskRow) {
  const description = cleanTaskText(task.description);
  if (!description || description === task.taskId) {
    return titleFromTaskId(task.taskId);
  }
  const colon = description.indexOf(":");
  if (colon > 8 && colon <= 72) {
    return sentenceCaseTitle(description.slice(0, colon));
  }
  const firstSentence = description.split(/[.!?]\s/)[0]?.trim();
  return sentenceCaseTitle(truncateText(firstSentence || description, 96));
}

function taskUsefulDescription(task: TaskRow) {
  const description = cleanTaskText(task.description);
  if (!description || description === task.taskId) {
    return "No description was recorded for this task.";
  }
  const colon = description.indexOf(":");
  if (colon > 8 && colon <= 72 && colon + 1 < description.length) {
    return truncateText(description.slice(colon + 1).trim(), 180);
  }
  return truncateText(description, 180);
}

function cleanTaskText(value: string) {
  return value
    .replace(/^Task:\s*/i, "")
    .replace(/^task:\s*/i, "")
    .replace(/\s+/g, " ")
    .trim();
}

function titleFromTaskId(taskId: string) {
  if (!taskId || taskId === "-") {
    return "-";
  }
  return sentenceCaseTitle(
    taskId
      .replace(/^\d{4}-\d{2}-\d{2}-/, "")
      .replace(/-[a-z0-9]{6}$/, "")
      .replaceAll("-", " ")
  );
}

function sentenceCaseTitle(value: string) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? "Task" : trimmed[0].toUpperCase() + trimmed.slice(1);
}

function truncateText(value: string, maxLength: number) {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 3).trimEnd()}...`;
}

function taskTimelineItems(events: BusEvent[]) {
  return events
    .map(taskTimelineItem)
    .sort((left, right) => right.updatedAt.getTime() - left.updatedAt.getTime());
}

function taskTimelineItem(event: BusEvent): TimelineItem {
  const eventType = stringField(event.envelope, "event_type") ?? event.subject;
  const actor = stringField(event.envelope, "actor") ?? "-";
  const payload = objectField(event.envelope, "payload");
  const base = {
    key: event.id,
    actor,
    updatedAt: event.receivedAt
  };

  if (eventType === "task.requested") {
    const title = taskDisplayName({
      ...emptyTaskRow(stringField(payload, "task_id") ?? "task", event.receivedAt),
      description: stringField(payload, "description") ?? "Task added"
    });
    const goal = taskUsefulDescription({
      ...emptyTaskRow(stringField(payload, "task_id") ?? "task", event.receivedAt),
      description: stringField(payload, "description") ?? "Task added"
    });
    return {
      ...base,
      status: "backlog",
      title: "Task added",
      detail: `${title}. Goal: ${goal}. Class ${stringField(payload, "task_class") ?? "-"}, priority ${stringField(payload, "priority") ?? "-"}.`
    };
  }
  if (eventType === "task.failed") {
    const reason = stringField(payload, "reason") ?? "task failure";
    const detail = stringField(payload, "detail");
    return {
      ...base,
      status: "failed",
      title: "Task failed",
      detail: detail ? `${reason}: ${detail}` : reason
    };
  }
  if (eventType === "picker.spawned") {
    return {
      ...base,
      status: "running",
      title: "Picker started",
      detail: `${stringField(payload, "harness") ?? "Picker"} opened session ${stringField(payload, "session_id") ?? "-"}`
    };
  }
  if (eventType === "picker.first-output") {
    return {
      ...base,
      status: "running",
      title: "Picker produced output",
      detail: `${stringField(payload, "session_id") ?? "The Picker"} started writing run output.`
    };
  }
  if (eventType === "picker.exited") {
    const exitCode = numberField(payload, "exit_code");
    const status = exitCode === 0 ? "picker-completed" : "failed";
    return {
      ...base,
      status,
      title: exitCode === 0 ? "Picker finished" : "Picker failed",
      detail:
        exitCode === 0
          ? `The Picker finished successfully${durationDetail(payload)}. Jamboree can now decide whether there are changes to hand off as a PR.`
          : `The Picker stopped with exit code ${exitCode ?? "unknown"}${durationDetail(payload)}. This task needs attention before it can continue.`
    };
  }
  if (eventType === "tempyr.task-updated") {
    const status = stringField(payload, "status") ?? "unknown";
    return {
      ...base,
      status,
      title: taskStateUpdateTitle(status),
      detail: taskStateUpdateDetail(status, stringField(payload, "source_event_type"))
    };
  }
  if (eventType === "worktree.created") {
    return {
      ...base,
      status: "running",
      title: "Worktree prepared",
      detail: `Created isolated workspace${stringField(payload, "project") ? ` for ${stringField(payload, "project")}` : ""}${stringField(payload, "worktree_path") ? ` at ${stringField(payload, "worktree_path")}` : ""}`
    };
  }
  if (eventType === "pr.opened") {
    return {
      ...base,
      status: boolField(payload, "draft") === true ? "draft" : "in-review",
      title: "Pull request opened",
      detail: `${prRefFromPayload(payload) ?? "PR"} is ${boolField(payload, "draft") === true ? "a draft waiting for review prep" : "ready for review"}.`
    };
  }
  if (eventType === "pr.status-changed") {
    const status = prStatusFromEvent(eventType, payload, "unknown");
    return {
      ...base,
      status,
      title: "Pull request state changed",
      detail: `${prRefFromPayload(payload) ?? "PR"} is now ${statusLabel(status)}.`
    };
  }
  if (eventType === "pr.ci.status-changed") {
    const status = ciStatusFromPayload(payload, "unknown");
    return {
      ...base,
      status,
      title: "CI updated",
      detail: `${prRefFromPayload(payload) ?? "PR"} CI is ${statusLabel(status)}.`
    };
  }
  if (eventType === "pr.review-received") {
    return {
      ...base,
      status: "review",
      title: "Review received",
      detail: reviewSummaryFromPayload(payload)
    };
  }
  if (eventType === "pr.merged") {
    return {
      ...base,
      status: "merged",
      title: "Pull request merged",
      detail: `${prRefFromPayload(payload) ?? "PR"} merged${stringField(payload, "merged_sha") ? ` at ${shortSha(stringField(payload, "merged_sha") ?? "")}` : ""}.`
    };
  }
  if (eventType === "quota.usage-observed") {
    return {
      ...base,
      status: "unknown",
      title: "Quota usage recorded",
      detail: `Harness usage changed: ${usageSummary(payload, {})}.`
    };
  }
  if (eventType === "task.abandoned") {
    return {
      ...base,
      status: "abandoned",
      title: "Task abandoned",
      detail: stringField(payload, "reason") ?? "No reason recorded"
    };
  }

  return {
    ...base,
    status: "unknown",
    title: readableEventType(eventType),
    detail: compactPayloadSummary(payload) || eventSummary(event)
  };
}

function eventUpdate(event: BusEvent): TimelineItem {
  const eventType = stringField(event.envelope, "event_type") ?? event.subject;
  if (
    isTaskEvent(eventType) ||
    eventType.startsWith("quota.") ||
    eventType.startsWith("pr.") ||
    eventType.startsWith("picker.")
  ) {
    return taskTimelineItem(event);
  }
  const payload = objectField(event.envelope, "payload");
  return {
    key: event.id,
    status: eventType.includes("failed") ? "failed" : "unknown",
    title: readableEventType(eventType),
    detail: compactPayloadSummary(payload) || event.subject,
    actor: stringField(event.envelope, "actor") ?? "-",
    updatedAt: event.receivedAt
  };
}

function durationDetail(payload: Record<string, unknown>) {
  const durationMs = numberField(payload, "duration_ms");
  if (durationMs === undefined) {
    return "";
  }
  if (durationMs < 1000) {
    return ` after ${durationMs} ms`;
  }
  return ` after ${Math.round(durationMs / 1000)} seconds`;
}

function taskStateUpdateTitle(status: string) {
  const normalized = normalizeStatus(status);
  if (normalized === "backlog") {
    return "Task queued";
  }
  if (normalized === "in-progress" || normalized === "running") {
    return "Task started";
  }
  if (normalized === "picker-completed") {
    return "Picker work completed";
  }
  if (normalized === "draft") {
    return "Draft PR opened";
  }
  if (normalized === "in-review" || normalized === "review") {
    return "Review in progress";
  }
  if (normalized === "merged") {
    return "Task merged";
  }
  if (normalized === "failed") {
    return "Task failed";
  }
  if (normalized === "abandoned") {
    return "Task abandoned";
  }
  return `Task status updated to ${status}`;
}

function taskStateUpdateDetail(status: string, sourceEventType: string | undefined) {
  const normalized = normalizeStatus(status);
  const source = sourceEventLabel(sourceEventType);
  if (normalized === "backlog") {
    return `The task is waiting in the backlog. Source: ${source}.`;
  }
  if (normalized === "in-progress" || normalized === "running") {
    return `A Picker has started work in an isolated workspace. Source: ${source}.`;
  }
  if (normalized === "picker-completed") {
    return `The Picker finished successfully. Jamboree is ready to hand off changes to a PR when there are code changes. Source: ${source}.`;
  }
  if (normalized === "draft") {
    return `A draft PR exists, but it is not ready for final review yet. Source: ${source}.`;
  }
  if (normalized === "in-review" || normalized === "review") {
    return `A PR or review is active. Source: ${source}.`;
  }
  if (normalized === "merged") {
    return `The PR was merged and this task is complete. Source: ${source}.`;
  }
  if (normalized === "failed") {
    return `The task failed and needs attention before it can continue. Source: ${source}.`;
  }
  if (normalized === "abandoned") {
    return `The task was explicitly abandoned and will not continue. Source: ${source}.`;
  }
  return `The task moved to ${statusLabel(status)}. Source: ${source}.`;
}

function sourceEventLabel(eventType: string | undefined) {
  if (!eventType) {
    return "journal event";
  }
  const labels: Record<string, string> = {
    "task.requested": "task request",
    "task.failed": "task failure",
    "picker.spawned": "Picker start",
    "picker.first-output": "Picker output",
    "picker.exited": "Picker exit",
    "pr.opened": "PR opened",
    "pr.status-changed": "PR status update",
    "pr.merged": "PR merged",
    "task.abandoned": "manual abandon"
  };
  return labels[eventType] ?? readableEventType(eventType);
}

function actorLabel(actor: string) {
  const labels: Record<string, string> = {
    "human:caleb": "Caleb",
    "jam-svc-session": "Session service",
    "jam-svc-worktree": "Worktree service",
    "jam-task-lifecycle": "Task lifecycle service",
    "jam-pr-poller": "PR poller",
    "jam-cli": "CLI"
  };
  return labels[actor] ?? actor.replace(/^human:/, "");
}

function shortSha(value: string) {
  return value.length > 12 ? value.slice(0, 12) : value;
}

function readableEventType(eventType: string) {
  return sentenceCaseTitle(eventType.replaceAll(".", " ").replaceAll("-", " "));
}

function serviceSort(left: ServiceRow, right: ServiceRow) {
  if (left.running !== right.running) {
    return left.running ? -1 : 1;
  }
  const leftReady = left.ready.toLowerCase() === "ready";
  const rightReady = right.ready.toLowerCase() === "ready";
  if (leftReady !== rightReady) {
    return leftReady ? -1 : 1;
  }
  return left.name.localeCompare(right.name);
}

function eventSummary(event: BusEvent) {
  const eventType = stringField(event.envelope, "event_type") ?? event.subject;
  const actor = stringField(event.envelope, "actor");
  const payload = objectField(event.envelope, "payload");
  const parts: string[] = [];

  if (eventType === "task.requested") {
    parts.push(stringField(payload, "description") ?? "Task requested");
    appendPart(parts, "task", stringField(payload, "task_id"));
    appendPart(parts, "priority", stringField(payload, "priority"));
  } else if (eventType === "task.failed") {
    parts.push(stringField(payload, "reason") ?? "Task failed");
    appendPart(parts, "task", stringField(payload, "task_id"));
    appendPart(parts, "detail", stringField(payload, "detail"));
  } else if (eventType === "picker.spawned") {
    parts.push("Picker spawned");
    appendPart(parts, "task", stringField(payload, "task_id"));
    appendPart(parts, "session", stringField(payload, "session_id"));
    appendPart(parts, "harness", stringField(payload, "harness"));
  } else if (eventType === "pr.opened") {
    parts.push(stringField(payload, "title") ?? "PR opened");
    appendPart(parts, "pr", prRefFromPayload(payload));
    appendPart(parts, "task", stringField(payload, "task_id"));
  } else if (eventType === "pr.merged") {
    parts.push("PR merged");
    appendPart(parts, "pr", prRefFromPayload(payload));
    appendPart(parts, "sha", stringField(payload, "merged_sha"));
  } else if (eventType === "pr.status-changed") {
    parts.push("PR status changed");
    appendPart(parts, "pr", prRefFromPayload(payload));
    appendPart(parts, "state", stringField(payload, "state") ?? stringField(payload, "status"));
  } else if (eventType === "pr.review-received") {
    parts.push("Review received");
    appendPart(parts, "pr", prRefFromPayload(payload));
    appendPart(parts, "review", reviewSummaryFromPayload(payload));
  } else if (eventType === "pr.ci.status-changed") {
    parts.push("CI status changed");
    appendPart(parts, "pr", prRefFromPayload(payload));
    appendPart(parts, "ci", ciStatusFromPayload(payload, "-"));
  } else if (eventType.startsWith("quota.")) {
    parts.push(eventType);
    appendPart(parts, "harness", stringField(payload, "harness"));
    appendPart(parts, "remaining", fractionField(payload, "remaining"));
  } else {
    parts.push(compactPayloadSummary(payload) || eventType);
  }

  appendPart(parts, "actor", actor);
  return parts.join(" | ");
}

function appendPart(parts: string[], label: string, value: string | undefined) {
  if (value) {
    parts.push(`${label}: ${value}`);
  }
}

function compactPayloadSummary(payload: Record<string, unknown>) {
  const parts: string[] = [];
  const description = stringField(payload, "description");
  if (description) {
    parts.push(taskDisplayName({ ...emptyTaskRow(stringField(payload, "task_id") ?? "task", new Date()), description }));
  }
  const taskId = stringField(payload, "task_id");
  if (taskId && !description) {
    parts.push(`Task: ${titleFromTaskId(taskId)}`);
  }
  const project = stringField(payload, "project");
  if (project) {
    parts.push(`Project: ${project}`);
  }
  const status = stringField(payload, "status");
  if (status) {
    parts.push(`Status: ${statusLabel(status)}`);
  }
  const summary = stringField(payload, "summary");
  if (summary) {
    parts.push(summary);
  }
  const source = stringField(payload, "source");
  if (source) {
    parts.push(`Source: ${source}`);
  }
  return parts.slice(0, 4).join(" | ");
}

function notificationFromEvent(event: BusEvent): HumanNotification {
  const payload = parseNotifyPayload(event.payload);
  const summary = typeof payload.summary === "string" ? payload.summary : event.payload;
  const urgency = normalizeUrgency(payload.urgency);
  const detail =
    payload.payload === undefined ? "" : JSON.stringify(payload.payload, null, 2) ?? "";

  return {
    id: event.id,
    subject: event.subject,
    urgency,
    summary,
    detail,
    receivedAt: latestTime(event.receivedAt)
  };
}

function parseNotifyPayload(raw: string): NotifyPayload {
  try {
    const parsed = JSON.parse(raw) as NotifyPayload;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return { summary: raw };
  }
}

function normalizeUrgency(value: unknown): HumanNotification["urgency"] {
  if (value === "low" || value === "medium" || value === "high" || value === "critical") {
    return value;
  }
  return "medium";
}

function parseObject(raw: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(raw);
    return recordFromUnknown(parsed);
  } catch {
    return {};
  }
}

function recordFromUnknown(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function objectField(
  value: Record<string, unknown>,
  key: string
): Record<string, unknown> {
  const field = value[key];
  return field && typeof field === "object" && !Array.isArray(field)
    ? (field as Record<string, unknown>)
    : {};
}

function stringField(value: Record<string, unknown>, key: string) {
  const field = value[key];
  return typeof field === "string" && field.length > 0 ? field : undefined;
}

function numberField(value: Record<string, unknown>, key: string) {
  const field = value[key];
  return typeof field === "number" && Number.isFinite(field) ? field : undefined;
}

function boolField(value: Record<string, unknown>, key: string) {
  const field = value[key];
  return typeof field === "boolean" ? field : undefined;
}

function fractionField(value: Record<string, unknown>, key: string) {
  const field = numberField(value, key);
  if (field === undefined) {
    return undefined;
  }
  return `${Math.round(field * 100)}%`;
}

function pickerStatus(eventType: string, existing: string) {
  if (eventType === "picker.spawned") {
    return "running";
  }
  if (eventType === "picker.exited") {
    return "exited";
  }
  if (eventType === "picker.killed") {
    return "killed";
  }
  return existing;
}

function quotaStatus(eventType: string, payload: Record<string, unknown>) {
  if (eventType === "quota.exhausted") {
    return "exhausted";
  }
  if (eventType === "quota.exhausted-soon") {
    return "low";
  }
  if (eventType === "quota.refilled") {
    return "available";
  }
  return stringField(payload, "status") ?? "unknown";
}

function usageSummary(payload: Record<string, unknown>, usage: Record<string, unknown>) {
  const input = numberField(payload, "input_tokens") ?? numberField(usage, "input_tokens") ?? 0;
  const output = numberField(payload, "output_tokens") ?? numberField(usage, "output_tokens") ?? 0;
  const cost = numberField(payload, "cost_usd") ?? numberField(usage, "cost_usd");
  const costPart = cost === undefined ? "" : ` $${cost.toFixed(4)}`;
  return `in ${input} / out ${output}${costPart}`;
}

function upsertOutboxStatus(
  items: MessageOutboxItem[],
  messageId: string,
  status: string,
  event: BusEvent
) {
  const payload = event.envelope;
  const sessionId = stringField(payload, "session_id") ?? "-";
  const mode = normalizeMessageMode(stringField(payload, "mode"));
  const detail = formatUnknown(objectField(payload, "detail"));
  let found = false;
  const updated = items.map((item) => {
    if (item.messageId !== messageId) {
      return item;
    }
    found = true;
    if (messageStatusRank(status) < messageStatusRank(item.status)) {
      return item;
    }
    return { ...item, status, detail, updatedAt: event.receivedAt };
  });
  if (found) {
    return updated;
  }
  return [
    {
      messageId,
      sessionId,
      mode,
      status,
      text: "",
      detail,
      updatedAt: event.receivedAt
    },
    ...items
  ].slice(0, 12);
}

function reconcilePromptAck(
  items: MessageOutboxItem[],
  pendingId: string,
  rendered: MessageResponse,
  text: string
) {
  const ackDetail = formatUnknown(rendered.detail);
  const pending = items.find((item) => item.messageId === pendingId);
  const existing = items.find((item) => item.messageId === rendered.message_id);
  const base: MessageOutboxItem = existing ?? pending ?? {
    messageId: rendered.message_id,
    sessionId: rendered.session_id,
    mode: rendered.mode,
    status: rendered.status,
    text,
    detail: ackDetail,
    updatedAt: new Date()
  };
  const keepExistingStatus =
    existing !== undefined &&
    messageStatusRank(existing.status) > messageStatusRank(rendered.status);
  const reconciled: MessageOutboxItem = {
    ...base,
    messageId: rendered.message_id,
    sessionId: rendered.session_id,
    mode: rendered.mode,
    status: keepExistingStatus ? base.status : rendered.status,
    text: base.text || text,
    detail: keepExistingStatus ? base.detail : ackDetail,
    updatedAt: keepExistingStatus ? base.updatedAt : new Date()
  };
  return [
    reconciled,
    ...items.filter(
      (item) => item.messageId !== pendingId && item.messageId !== rendered.message_id
    )
  ].slice(0, 12);
}

function messageStatusRank(status: string) {
  const ranks: Record<string, number> = {
    sending: 0,
    queued: 1,
    "interrupt-requested": 1,
    "kill-requested": 1,
    "interrupt-accepted": 2,
    delivered: 3,
    "kill-confirmed": 3,
    acknowledged: 4,
    "delivery-failed": 5
  };
  return ranks[status] ?? 0;
}

function isPendingDeliveryStatus(status: string) {
  return ["sending", "queued", "interrupt-requested", "interrupt-accepted"].includes(status);
}

function conciseDetail(detail: unknown) {
  const trimmed = typeof detail === "string" ? detail.trim() : "";
  if (!trimmed || trimmed === "{}") {
    return "";
  }
  try {
    const parsed = JSON.parse(trimmed) as unknown;
    const record = recordFromUnknown(parsed);
    return (
      stringField(record, "error") ??
      stringField(record, "detail") ??
      stringField(record, "message") ??
      trimmed
    );
  } catch {
    return trimmed;
  }
}

function normalizeMessageMode(value: string | undefined): MessageMode {
  if (value === "interrupt" || value === "full-stop") {
    return value;
  }
  return "queue";
}

function modeLabel(mode: MessageMode) {
  if (mode === "full-stop") {
    return "Full-stop";
  }
  return mode[0].toUpperCase() + mode.slice(1);
}

function parseLogLine(record: SessionOutputRecord): ParsedLogLine {
  const raw = record.line.trim();
  if (!raw) {
    return {
      kind: "text",
      title: "Output",
      body: "",
      meta: []
    };
  }
  const parsed = parseJsonLike(raw);
  if (!parsed) {
    return {
      kind: record.stream === "stderr" ? "error" : "text",
      title: record.stream === "stderr" ? "Error output" : "Output",
      body: cleanModelText(raw),
      meta: [`seq ${record.sequence}`]
    };
  }
  const event = recordFromUnknown(parsed);
  const title = logTitle(event, record);
  const summary = compactPayloadSummary(logPayload(event));
  const body = logBody(event) || summary || readableEventType(logType(event)) || "Event recorded.";
  return {
    kind: logKind(event, record),
    title,
    body,
    meta: logMeta(event, record)
  };
}

function parseJsonLike(raw: string): unknown | undefined {
  try {
    return JSON.parse(raw);
  } catch {
    const start = raw.indexOf("{");
    const end = raw.lastIndexOf("}");
    if (start >= 0 && end > start) {
      try {
        return JSON.parse(raw.slice(start, end + 1));
      } catch {
        return undefined;
      }
    }
    return undefined;
  }
}

function logKind(event: Record<string, unknown>, record: SessionOutputRecord): ParsedLogLine["kind"] {
  const type = logType(event).toLowerCase();
  const payload = logPayload(event);
  const itemType = stringField(payload, "type")?.toLowerCase() ?? "";
  const role = (stringField(event, "role") ?? stringField(payload, "role"))?.toLowerCase();
  if (record.stream === "stderr" || type.includes("error") || stringField(event, "error")) {
    return "error";
  }
  if (role === "user" || itemType === "user_message") {
    return "user";
  }
  if (role === "assistant" || type.includes("assistant") || type.includes("message")) {
    return "assistant";
  }
  if (
    itemType === "command_execution" ||
    type.includes("tool") ||
    type.includes("command") ||
    stringField(event, "tool_name") ||
    stringField(payload, "tool_name")
  ) {
    return "tool";
  }
  if (type.includes("status") || type.includes("turn") || type.includes("result")) {
    return "status";
  }
  return "text";
}

function logTitle(event: Record<string, unknown>, record: SessionOutputRecord) {
  const type = logType(event);
  const payload = logPayload(event);
  const role = stringField(event, "role") ?? stringField(payload, "role");
  const payloadType = stringField(payload, "type");
  const tool = stringField(event, "tool_name") ?? stringField(payload, "tool_name") ?? stringField(event, "name");
  if (tool) {
    return `Tool: ${tool}`;
  }
  if (payloadType === "command_execution") {
    return commandTitle(payload);
  }
  if (payloadType === "agent_message" || role === "assistant") {
    return "Assistant";
  }
  if (role === "assistant") {
    return "Assistant";
  }
  if (role === "user") {
    return "User message";
  }
  if (type) {
    return readableEventType(type);
  }
  return record.stream === "stderr" ? "Error output" : "Output";
}

function logBody(event: Record<string, unknown>): string {
  const payload = logPayload(event);
  const payloadType = stringField(payload, "type");
  if (payloadType === "command_execution") {
    return commandBody(payload);
  }
  if (payloadType === "agent_message") {
    const text = stringField(payload, "text");
    if (text) {
      return cleanModelText(text);
    }
  }

  const direct =
    stringField(event, "text") ??
    stringField(payload, "text") ??
    stringField(event, "message") ??
    stringField(payload, "message") ??
    stringField(event, "content") ??
    stringField(payload, "content") ??
    stringField(event, "delta") ??
    stringField(payload, "delta") ??
    stringField(event, "output") ??
    stringField(payload, "output") ??
    stringField(event, "result") ??
    stringField(payload, "result") ??
    stringField(event, "summary") ??
    stringField(payload, "summary") ??
    stringField(event, "error");
  if (direct) {
    return cleanModelText(direct);
  }
  const nestedMessage = objectField(event, "message");
  const nestedContent = stringField(nestedMessage, "content") ?? stringField(nestedMessage, "text");
  if (nestedContent) {
    return cleanModelText(nestedContent);
  }
  const content = event.content;
  if (Array.isArray(content)) {
    return content.map(contentPartText).filter(Boolean).join("\n\n");
  }
  if (Array.isArray(payload.content)) {
    return payload.content.map(contentPartText).filter(Boolean).join("\n\n");
  }
  const nested = extractTextFromUnknown(payload);
  if (nested) {
    return nested;
  }
  const result = event.result;
  if (result && typeof result === "object") {
    return compactPayloadSummary(result as Record<string, unknown>);
  }
  return "";
}

function contentPartText(part: unknown): string {
  if (typeof part === "string") {
    return cleanModelText(part);
  }
  const record = recordFromUnknown(part);
  return cleanModelText(
    stringField(record, "text") ??
      stringField(record, "content") ??
      stringField(record, "message") ??
      ""
  );
}

function logType(event: Record<string, unknown>) {
  return (
    stringField(event, "type") ??
    stringField(event, "event") ??
    stringField(event, "event_type") ??
    stringField(event, "kind") ??
    ""
  );
}

function logPayload(event: Record<string, unknown>) {
  const item = objectField(event, "item");
  if (Object.keys(item).length > 0) {
    return item;
  }
  const payload = objectField(event, "payload");
  if (Object.keys(payload).length > 0) {
    return payload;
  }
  const data = objectField(event, "data");
  if (Object.keys(data).length > 0) {
    return data;
  }
  return event;
}

function commandTitle(payload: Record<string, unknown>) {
  const status = stringField(payload, "status");
  if (status === "in_progress") {
    return "Command started";
  }
  if (status === "completed") {
    return "Command completed";
  }
  return "Command";
}

function commandBody(payload: Record<string, unknown>) {
  const parts: string[] = [];
  const command = stringField(payload, "command");
  if (command) {
    parts.push(`$ ${command}`);
  }
  const output = stringField(payload, "aggregated_output");
  if (output) {
    parts.push(cleanModelText(output));
  }
  const exitCode = numberField(payload, "exit_code");
  if (exitCode !== undefined && exitCode !== 0) {
    parts.push(`exit code ${exitCode}`);
  }
  return parts.join("\n\n");
}

function extractTextFromUnknown(value: unknown, depth = 0): string {
  if (depth > 4 || value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return cleanModelText(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => extractTextFromUnknown(item, depth + 1)).filter(Boolean).join("\n\n");
  }
  if (typeof value !== "object") {
    return "";
  }
  const record = value as Record<string, unknown>;
  const direct =
    stringField(record, "text") ??
    stringField(record, "content") ??
    stringField(record, "message") ??
    stringField(record, "delta") ??
    stringField(record, "output") ??
    stringField(record, "result") ??
    stringField(record, "summary");
  if (direct) {
    return cleanModelText(direct);
  }
  for (const key of ["content", "message", "payload", "item", "data", "result", "output"]) {
    const nested = extractTextFromUnknown(record[key], depth + 1);
    if (nested) {
      return nested;
    }
  }
  return "";
}

function logMeta(event: Record<string, unknown>, record: SessionOutputRecord) {
  const meta: string[] = [`seq ${record.sequence}`];
  const payload = logPayload(event);
  appendPart(meta, "type", logType(event));
  appendPart(meta, "role", stringField(event, "role") ?? stringField(payload, "role"));
  appendPart(meta, "model", stringField(event, "model") ?? stringField(payload, "model"));
  appendPart(meta, "status", stringField(event, "status") ?? stringField(payload, "status"));
  appendPart(meta, "tool", stringField(event, "tool_name") ?? stringField(payload, "tool_name") ?? stringField(event, "name"));
  const usage = objectField(event, "usage");
  const usageText = usageSummary(event, usage);
  if (usageText !== "in 0 / out 0") {
    meta.push(usageText);
  }
  return meta.slice(0, 8);
}

function cleanModelText(value: string) {
  return value
    .replace(/\\n/g, "\n")
    .replace(/\\"/g, '"')
    .replace(/\s+$/g, "")
    .trim();
}

function formatPayload(event: BusEvent) {
  if (Object.keys(event.envelope).length === 0) {
    return event.payload;
  }
  return JSON.stringify(event.envelope, null, 2);
}

function formatUnknown(value: unknown) {
  if (value === undefined || value === null) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  return JSON.stringify(value, null, 2) ?? "";
}

function loadTheme(): ThemeMode {
  const stored = localStorage.getItem("jam.ui.theme");
  if (stored === "dark" || stored === "light" || stored === "system") {
    return stored;
  }
  // Default to following the OS — same behavior as the OS-level system
  // pref, no surprise toggle on first visit.
  return "system";
}

/**
 * Resolve a `ThemeMode` to the actual `"light"` / `"dark"` palette to apply.
 * `system` defers to `prefers-color-scheme`; everything else is identity.
 */
function effectiveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "light" || mode === "dark") return mode;
  if (typeof window === "undefined" || !window.matchMedia) return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function parseDate(value: string | undefined) {
  if (!value || value === "-") {
    return new Date(0);
  }
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? new Date(0) : parsed;
}

function latestTime(value: Date | undefined) {
  return value && value.getTime() > 0 ? value.toLocaleString() : "-";
}

function runLogTimestamp(value: Date | undefined) {
  if (!value || value.getTime() <= 0) {
    return "-";
  }
  return `${relativeTime(value)} (${compactDateTime(value)})`;
}

function relativeTime(value: Date) {
  const elapsedSeconds = Math.max(0, Math.round((Date.now() - value.getTime()) / 1000));
  if (elapsedSeconds < 60) {
    return `${elapsedSeconds}s ago`;
  }
  const elapsedMinutes = Math.round(elapsedSeconds / 60);
  if (elapsedMinutes < 60) {
    return `${elapsedMinutes}m ago`;
  }
  const elapsedHours = Math.round(elapsedMinutes / 60);
  if (elapsedHours < 24) {
    return `${elapsedHours}h ago`;
  }
  const elapsedDays = Math.round(elapsedHours / 24);
  if (elapsedDays < 14) {
    return `${elapsedDays}d ago`;
  }
  const elapsedWeeks = Math.round(elapsedDays / 7);
  if (elapsedWeeks < 10) {
    return `${elapsedWeeks}w ago`;
  }
  return `${elapsedDays}d ago`;
}

function compactDateTime(value: Date) {
  const time = value
    .toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    .replace(/\s/g, "")
    .toLowerCase();
  const date = value.toLocaleDateString(undefined, {
    month: "numeric",
    day: "numeric",
    year: "numeric"
  });
  return `${time} ${date}`;
}

function normalizePath(path: string) {
  return path.length > 1 && path.endsWith("/") ? path.slice(0, -1) : path;
}

function traceIdFromPath(path: string) {
  const match = path.match(/^\/traces\/([^/]+)$/);
  return match ? decodeURIComponent(match[1]) : undefined;
}

render(() => <App />, document.getElementById("root") as HTMLElement);
