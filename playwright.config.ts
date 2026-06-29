import { defineConfig, devices } from "@playwright/test";
import { execSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "goatns-e2e-"));
const certDir = path.join(tmpDir, "certs");
fs.mkdirSync(certDir, { recursive: true });

const rootDir = process.cwd();
const scriptPath = path.join(rootDir, "insecure_generate_tls.sh");
execSync(`CERT_DIR=${certDir} bash ${scriptPath}`);

const dbPath = path.join(tmpDir, "test.sqlite");
const apiPort = 18_779;

const config = {
  addr: "127.0.0.1",
  hostname: "localhost",
  port: 15_354,
  log_level: "INFO",
  capture_packets: false,
  enable_hinfo: false,
  enable_api: true,
  api_port: apiPort,
  api_tls_cert: path.join(certDir, "cert.pem"),
  api_tls_key: path.join(certDir, "key.pem"),
  api_static_dir: path.join(rootDir, "static_files"),
  sqlite_path: dbPath,
  allowed_tlds: [],
  user_auto_provisioning: false,
  shutdown_ip_allow_list: ["127.0.0.1"],
};

const configPath = path.join(tmpDir, "goatns.json");
fs.writeFileSync(configPath, JSON.stringify(config, null, 2));

process.env.GOATNS_E2E_CONFIG_PATH = configPath;

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 120_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `https://localhost:${apiPort}`,
    trace: "on-failure",
    ignoreHTTPSErrors: true,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `cargo build --bin test_login --bin goatns && cargo run --bin goatns -- server`,
    cwd: rootDir,
    env: {
      GOATNS_CONFIG_FILE: configPath,
    },
    port: apiPort,
    timeout: 180_000,
    reuseExistingServer: false,
    ignoreHTTPSErrors: true,
  },
});
