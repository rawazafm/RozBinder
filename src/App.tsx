import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import "./App.css";

type ListedFile = {
  path: string;
  name: string;
  size: number;
};

type DescribeResult = {
  files: ListedFile[];
  errors: string[];
};

type MergeReport = {
  output: string;
  files: number;
  bytes: number;
  names: string[];
  icon: boolean;
};

type IconChoice = {
  path: string;
  name: string;
};

const filters = [{ name: "Executable", extensions: ["exe"] }];

function App() {
  const [items, setItems] = useState<ListedFile[]>([]);
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [error, setError] = useState("");
  const [report, setReport] = useState<MergeReport | null>(null);
  const [icon, setIcon] = useState<IconChoice | null>(null);
  const addPathsRef = useRef<(paths: string[]) => Promise<void>>(async () => {});

  async function addPaths(paths: string[]) {
    const exePaths = paths.filter((path) => path.toLowerCase().endsWith(".exe"));
    const icoPaths = paths.filter((path) => path.toLowerCase().endsWith(".ico"));
    const skipped = paths.length - exePaths.length - icoPaths.length;
    const messages: string[] = [];

    if (icoPaths.length > 0) {
      try {
        const chosen = await invoke<IconChoice>("inspect_icon", {
          path: icoPaths[icoPaths.length - 1],
        });
        setIcon(chosen);
        setReport(null);
      } catch (err) {
        messages.push(errorText(err));
      }
    }

    if (exePaths.length === 0) {
      if (skipped > 0) messages.push("Drop .exe or .ico files.");
      if (messages.length === 0 && icoPaths.length === 0) {
        setError("No files selected.");
      } else {
        setError(messages.join("\n"));
      }
      return;
    }

    try {
      const described = await invoke<DescribeResult>("describe_files", { paths: exePaths });
      setItems((current) => {
        const seen = new Set(current.map((item) => item.path.toLowerCase()));
        const next = [...current];
        for (const file of described.files) {
          const key = file.path.toLowerCase();
          if (seen.has(key)) continue;
          seen.add(key);
          next.push(file);
        }
        return next;
      });
      if (described.files[0]) {
        setOutput((current) => (current.trim() ? current : defaultOutput(described.files[0].path)));
      }
      messages.push(...described.errors);
      if (skipped > 0) messages.push("Skipped files that are not executables or icons.");
      setError(messages.join("\n"));
      setReport(null);
    } catch (err) {
      messages.push(errorText(err));
      setError(messages.join("\n"));
    }
  }

  addPathsRef.current = addPaths;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let webview;
    try {
      webview = getCurrentWebview();
    } catch {
      return;
    }
    webview
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragOver(true);
          return;
        }
        if (event.payload.type === "leave") {
          setDragOver(false);
          return;
        }
        if (event.payload.type === "drop") {
          setDragOver(false);
          void addPathsRef.current(event.payload.paths);
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  async function browseFiles() {
    const picked = await open({
      multiple: true,
      filters,
      title: "Add executables",
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await addPaths(paths);
  }

  async function browseIcon() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Icon", extensions: ["ico"] }],
      title: "Choose an icon",
    });
    if (!picked || Array.isArray(picked)) return;
    try {
      const chosen = await invoke<IconChoice>("inspect_icon", { path: picked });
      setIcon(chosen);
      setReport(null);
      setError("");
    } catch (err) {
      setError(errorText(err));
    }
  }

  async function browseOutput() {
    const picked = await save({
      defaultPath: output || "merged.exe",
      filters,
      title: "Save merged executable",
    });
    if (!picked) return;
    setOutput(picked);
    setReport(null);
  }

  async function merge() {
    setBusy(true);
    setError("");
    setReport(null);
    try {
      const result = await invoke<MergeReport>("merge_exes", {
        inputs: items.map((item) => item.path),
        output,
        icon: icon?.path ?? null,
      });
      setOutput(result.output);
      setReport(result);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  async function reveal() {
    if (!report) return;
    try {
      await revealItemInDir(report.output);
    } catch (err) {
      setError(errorText(err));
    }
  }

  function move(index: number, direction: -1 | 1) {
    setItems((current) => {
      const target = index + direction;
      if (target < 0 || target >= current.length) return current;
      const next = current.slice();
      const [item] = next.splice(index, 1);
      next.splice(target, 0, item);
      return next;
    });
    setReport(null);
  }

  function remove(index: number) {
    setItems((current) => current.filter((_, itemIndex) => itemIndex !== index));
    setReport(null);
  }

  const totalBytes = items.reduce((sum, item) => sum + item.size, 0);
  const canMerge = items.length > 0 && output.trim().length > 0 && !busy;

  return (
    <main className="app">
      <header className="top">
        <div>
          <p className="eyebrow">Executable merger</p>
          <h1>Binder</h1>
        </div>
        <div className="header-actions">
          <button type="button" onClick={() => void browseFiles()} disabled={busy}>
            Add executables
          </button>
          {items.length > 0 && (
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setItems([]);
                setReport(null);
                setError("");
              }}
              disabled={busy}
            >
              Clear
            </button>
          )}
        </div>
      </header>

      <section className={dragOver ? "panel over" : "panel"}>
        {items.length === 0 ? (
          <button type="button" className="empty" onClick={() => void browseFiles()}>
            <strong>Drop executables here</strong>
            <span>
              The merged file is a launcher. It unpacks the programs into a temporary folder and
              starts them in the order you list. Drop an .ico file to set its icon.
            </span>
          </button>
        ) : (
          <>
            <div className="list-head">
              <span>
                {items.length} {items.length === 1 ? "file" : "files"} · {formatSize(totalBytes)}
              </span>
              <span>Starts from top to bottom</span>
            </div>
            <ul className="files">
              {items.map((item, index) => (
                <li key={item.path} className="file">
                  <span className="index">{index + 1}</span>
                  <div className="identity">
                    <span className="name">{item.name}</span>
                    <span className="path" title={item.path}>
                      {item.path}
                    </span>
                  </div>
                  <span className="size">{formatSize(item.size)}</span>
                  <div className="row-actions">
                    <button
                      type="button"
                      aria-label={`Move ${item.name} up`}
                      disabled={busy || index === 0}
                      onClick={() => move(index, -1)}
                    >
                      Up
                    </button>
                    <button
                      type="button"
                      aria-label={`Move ${item.name} down`}
                      disabled={busy || index === items.length - 1}
                      onClick={() => move(index, 1)}
                    >
                      Down
                    </button>
                    <button
                      type="button"
                      className="remove"
                      aria-label={`Remove ${item.name}`}
                      disabled={busy}
                      onClick={() => remove(index)}
                    >
                      Remove
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>

      <form
        className="save-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (canMerge) void merge();
        }}
      >
        <div className="save">
          <label className="output">
            <span>Icon</span>
            <input
              readOnly
              value={icon?.path ?? ""}
              spellCheck={false}
              placeholder="Optional .ico for the merged executable"
              title={icon?.path ?? ""}
            />
          </label>
          <button type="button" onClick={() => void browseIcon()} disabled={busy}>
            Set icon
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => {
              setIcon(null);
              setReport(null);
            }}
            disabled={busy || !icon}
          >
            Remove
          </button>
        </div>
        <div className="save">
        <label className="output">
          <span>Output</span>
          <input
            value={output}
            spellCheck={false}
            autoComplete="off"
            placeholder="Choose where to save the merged .exe"
            onChange={(event) => {
              setOutput(event.currentTarget.value);
              setReport(null);
            }}
          />
        </label>
        <button type="button" onClick={() => void browseOutput()} disabled={busy}>
          Browse
        </button>
        <button type="submit" className="primary" disabled={!canMerge}>
          {busy ? "Merging…" : "Merge"}
        </button>
        </div>
      </form>

      <div className="status" aria-live="polite">
        {error ? (
          <p className="message error">{error}</p>
        ) : report ? (
          <p className="message ok">
            <span>
              Saved {report.files} {report.files === 1 ? "program" : "programs"}. Running{" "}
              {fileName(report.output)} unpacks them to a temporary folder and starts each one.
              {report.icon ? " The selected icon is embedded in the launcher." : ""}
            </span>
            <button type="button" onClick={() => void reveal()}>
              Show in folder
            </button>
          </p>
        ) : (
          <p className="message hint">
            Original files stay where they are. The launcher runs its own copies from a temporary
            folder.
          </p>
        )}
      </div>
    </main>
  );
}

function defaultOutput(filePath: string): string {
  const slash = Math.max(filePath.lastIndexOf("\\"), filePath.lastIndexOf("/"));
  if (slash <= 0) return "merged.exe";
  let dir = filePath.slice(0, slash);
  if (/^[A-Za-z]:$/.test(dir)) dir += "\\";
  const separator = filePath.includes("\\") ? "\\" : "/";
  return `${dir}${dir.endsWith("\\") || dir.endsWith("/") ? "" : separator}merged.exe`;
}

function fileName(filePath: string): string {
  const slash = Math.max(filePath.lastIndexOf("\\"), filePath.lastIndexOf("/"));
  return slash >= 0 ? filePath.slice(slash + 1) : filePath;
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.max(0, Math.round(bytes))} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Something went wrong.";
}

export default App;
