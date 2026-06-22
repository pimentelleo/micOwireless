import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type ReceiverStatus = {
  running: boolean;
  packetsReceived: number;
  packetsDropped: number;
  droppedSamples: number;
  decryptFailures: number;
  parseErrors: number;
  lastError: string | null;
};

const receiverSampleRate = 48_000;

const defaultStatus: ReceiverStatus = {
  running: false,
  packetsReceived: 0,
  packetsDropped: 0,
  droppedSamples: 0,
  decryptFailures: 0,
  parseErrors: 0,
  lastError: null,
};

function generatePairCode(length = 8): string {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  const random = crypto.getRandomValues(new Uint32Array(length));
  return Array.from(random)
    .map((value) => alphabet[value % alphabet.length])
    .join("");
}

function App() {
  const [port, setPort] = useState("49000");
  const [outputDevices, setOutputDevices] = useState<string[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [localIps, setLocalIps] = useState<string[]>([]);
  const [secureMode, setSecureMode] = useState(true);
  const [pairCode, setPairCode] = useState(() => generatePairCode());
  const [jitterStartupPackets, setJitterStartupPackets] = useState("4");
  const [jitterMaxPendingPackets, setJitterMaxPendingPackets] = useState("32");
  const [maxBufferMs, setMaxBufferMs] = useState("5000");
  const [status, setStatus] = useState<ReceiverStatus>(defaultStatus);
  const [notice, setNotice] = useState(
    "Start receiver, then use Discover in the mobile app.",
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const bootstrap = async () => {
      try {
        const [devices, ips] = await Promise.all([
          invoke<string[]>("list_output_devices"),
          invoke<string[]>("list_local_ipv4"),
        ]);
        setOutputDevices(devices);
        setLocalIps(ips);
        setSelectedDevice((current) => {
          if (current || devices.length === 0) {
            return current;
          }
          return devices[0];
        });
      } catch (err) {
        setError(String(err));
      }
    };

    const updateStatus = async () => {
      try {
        const current = await invoke<ReceiverStatus>("receiver_status");
        setStatus(current);
      } catch (err) {
        setError(String(err));
      }
    };

    bootstrap();
    updateStatus();
    const timer = window.setInterval(updateStatus, 500);
    return () => window.clearInterval(timer);
  }, []);

  const selectedDeviceLabel = useMemo(() => {
    if (!selectedDevice) {
      return "Default system output";
    }
    return selectedDevice;
  }, [selectedDevice]);

  const parsedPort = useMemo(() => Number(port), [port]);
  const discoveryPort = useMemo(() => parsedPort + 1, [parsedPort]);
  const parsedJitterStartupPackets = useMemo(
    () => Number(jitterStartupPackets),
    [jitterStartupPackets],
  );
  const parsedJitterMaxPendingPackets = useMemo(
    () => Number(jitterMaxPendingPackets),
    [jitterMaxPendingPackets],
  );
  const parsedMaxBufferMs = useMemo(() => Number(maxBufferMs), [maxBufferMs]);
  const parsedMaxBufferSamples = useMemo(
    () => Math.round((receiverSampleRate * parsedMaxBufferMs) / 1000),
    [parsedMaxBufferMs],
  );

  const handleStart = async () => {
    if (!Number.isInteger(parsedPort) || parsedPort < 1 || parsedPort > 65534) {
      setError("UDP port must be between 1 and 65534.");
      return;
    }
    if (secureMode && pairCode.trim().length < 6) {
      setError("Pair code must contain at least 6 characters.");
      return;
    }
    if (
      !Number.isInteger(parsedJitterStartupPackets) ||
      parsedJitterStartupPackets < 1 ||
      parsedJitterStartupPackets > 24
    ) {
      setError("Jitter startup packets must be between 1 and 24.");
      return;
    }
    if (
      !Number.isInteger(parsedJitterMaxPendingPackets) ||
      parsedJitterMaxPendingPackets < 4 ||
      parsedJitterMaxPendingPackets > 256
    ) {
      setError("Jitter max pending packets must be between 4 and 256.");
      return;
    }
    if (parsedJitterStartupPackets > parsedJitterMaxPendingPackets) {
      setError("Jitter startup packets cannot exceed max pending packets.");
      return;
    }
    if (
      !Number.isFinite(parsedMaxBufferMs) ||
      parsedMaxBufferMs < 100 ||
      parsedMaxBufferMs > 10000
    ) {
      setError("Max output buffer must be between 100 and 10000 ms.");
      return;
    }

    setError(null);
    try {
      await invoke("start_receiver", {
        port: parsedPort,
        deviceName: selectedDevice || null,
        secureMode,
        pairCode: secureMode ? pairCode.trim() : null,
        receiverTuning: {
          jitterStartupPackets: parsedJitterStartupPackets,
          jitterMaxPendingPackets: parsedJitterMaxPendingPackets,
          maxBufferSamples: parsedMaxBufferSamples,
        },
      });
      setNotice(
        `Receiver online on UDP ${parsedPort} (jitter ${parsedJitterStartupPackets}/${parsedJitterMaxPendingPackets}, buffer ${parsedMaxBufferMs} ms). Discovery listens on UDP ${discoveryPort}.`,
      );
    } catch (err) {
      setError(String(err));
    }
  };

  const handleStop = async () => {
    setError(null);
    try {
      await invoke("stop_receiver");
      setNotice("Receiver stopped.");
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <main className="app-shell">
      <div className="background">
        <div className="blob blob-a" />
        <div className="blob blob-b" />
      </div>

      <section className="glass-card header-card">
        <h1>micOwireless Desktop</h1>
        <p>
          Encrypted wireless audio receiver with discovery + jitter-tolerant
          playback.
        </p>
      </section>

      <section className="glass-card">
        <h2>Connection</h2>
        <label htmlFor="udp-port">UDP Port</label>
        <input
          id="udp-port"
          value={port}
          onChange={(event) => setPort(event.currentTarget.value)}
          placeholder="49000"
          inputMode="numeric"
        />
        <small>
          Mobile discovery scans UDP <b>{Number.isFinite(discoveryPort) ? discoveryPort : "-"}</b>.
        </small>

        <label htmlFor="device-select">Output Device</label>
        <select
          id="device-select"
          value={selectedDevice}
          onChange={(event) => setSelectedDevice(event.currentTarget.value)}
        >
          {outputDevices.length === 0 ? (
            <option value="">Default system output</option>
          ) : (
            outputDevices.map((device) => (
              <option key={device} value={device}>
                {device}
              </option>
            ))
          )}
        </select>

        <div className="button-row">
          <button
            className="primary"
            type="button"
            onClick={handleStart}
            disabled={status.running}
          >
            Start Receiver
          </button>
          <button type="button" onClick={handleStop} disabled={!status.running}>
            Stop Receiver
          </button>
        </div>
      </section>

      <section className="glass-card">
        <h2>Receiver Tuning (Experimental)</h2>
        <div className="settings-grid">
          <div>
            <label htmlFor="jitter-startup-packets">Jitter Startup Packets</label>
            <input
              id="jitter-startup-packets"
              value={jitterStartupPackets}
              onChange={(event) => setJitterStartupPackets(event.currentTarget.value)}
              placeholder="4"
              inputMode="numeric"
              disabled={status.running}
            />
            <small>
              Lower = lower latency, higher = smoother under unstable Wi-Fi.
            </small>
          </div>
          <div>
            <label htmlFor="jitter-max-pending">Jitter Max Pending Packets</label>
            <input
              id="jitter-max-pending"
              value={jitterMaxPendingPackets}
              onChange={(event) => setJitterMaxPendingPackets(event.currentTarget.value)}
              placeholder="32"
              inputMode="numeric"
              disabled={status.running}
            />
            <small>
              Safety window for out-of-order packets before silence insertion.
            </small>
          </div>
          <div>
            <label htmlFor="max-buffer-ms">Max Output Buffer (ms)</label>
            <input
              id="max-buffer-ms"
              value={maxBufferMs}
              onChange={(event) => setMaxBufferMs(event.currentTarget.value)}
              placeholder="5000"
              inputMode="numeric"
              disabled={status.running}
            />
            <small>
              Higher values prevent crackling on bad networks but increase delay.
            </small>
          </div>
        </div>
      </section>

      <section className="glass-card">
        <h2>Security & Pairing</h2>
        <label className="switch-row">
          <input
            type="checkbox"
            checked={secureMode}
            onChange={(event) => setSecureMode(event.currentTarget.checked)}
            disabled={status.running}
          />
          Secure mode (ChaCha20-Poly1305)
        </label>
        <label htmlFor="pair-code">Pair Code</label>
        <div className="pair-row">
          <input
            id="pair-code"
            value={pairCode}
            onChange={(event) => setPairCode(event.currentTarget.value)}
            disabled={status.running}
          />
          <button
            type="button"
            onClick={() => setPairCode(generatePairCode())}
            disabled={status.running}
          >
            Regenerate
          </button>
        </div>
        <small>Use this same code on the mobile app before starting stream.</small>
      </section>

      <section className="glass-card">
        <h2>Desktop IPs</h2>
        <div className="chip-row">
          {localIps.map((ip) => (
            <span key={ip} className="chip">
              {ip}
            </span>
          ))}
        </div>
      </section>

      <section className="glass-card">
        <h2>Receiver Status</h2>
        <div className="chip-row">
          <span className={`chip ${status.running ? "chip-live" : ""}`}>
            {status.running ? "Live" : "Idle"}
          </span>
          <span className="chip">Packets In: {status.packetsReceived}</span>
          <span className="chip">Packets Dropped: {status.packetsDropped}</span>
          <span className="chip">Samples Dropped: {status.droppedSamples}</span>
          <span className="chip">Decrypt Failures: {status.decryptFailures}</span>
          <span className="chip">Parse Errors: {status.parseErrors}</span>
          <span className="chip">Output: {selectedDeviceLabel}</span>
          <span className="chip">
            Jitter: {jitterStartupPackets}/{jitterMaxPendingPackets}
          </span>
          <span className="chip">Buffer: {maxBufferMs} ms</span>
        </div>
        <p>{notice}</p>
        {status.lastError ? <p className="error">{status.lastError}</p> : null}
        {error ? <p className="error">{error}</p> : null}
      </section>
    </main>
  );
}

export default App;
