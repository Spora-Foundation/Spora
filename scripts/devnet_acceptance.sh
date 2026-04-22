#!/usr/bin/env bash
set -Eeuo pipefail

PROFILE="smoke"
KEEP_ARTIFACTS=0
ACCEPTANCE_COMMAND="$0 $*"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      PROFILE="${2:?missing value for --profile}"
      shift 2
      ;;
    --profile=*)
      PROFILE="${1#*=}"
      shift
      ;;
    --full)
      PROFILE="full"
      shift
      ;;
    --keep-artifacts)
      KEEP_ARTIFACTS=1
      shift
      ;;
    -h|--help)
      cat <<'USAGE'
Usage: scripts/devnet_acceptance.sh [--profile smoke|external-boot|cellscript|propagation|full] [--keep-artifacts]

Profiles:
  smoke          Run the in-process devnet acceptance smoke test, including VM code-cell and CellScript ELF deploy/spend.
  external-boot  Generate a devnet wallet manifest and boot a real sporad process with explicit relaxed mass policy.
  cellscript     Run focused CellScript examples and package-manager acceptance tests.
  propagation    Run the two-node block/transaction propagation acceptance test.
  full           Run smoke, external-boot, propagation, and the focused CellScript tool/package suite.
USAGE
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
STARTED_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RUN_DIR="$REPO_ROOT/target/devnet-acceptance/$RUN_ID"
BOOTSTRAP_JSON="$RUN_DIR/bootstrap.json"
SPORAD_LOG="$RUN_DIR/sporad.log"
PROBE_JSON="$RUN_DIR/probe-report.json"
SMOKE_REPORT_JSON="$RUN_DIR/smoke-report.json"
CELLSCRIPT_REPORT_JSON="$RUN_DIR/cellscript-report.json"
PROPAGATION_REPORT_JSON="$RUN_DIR/propagation-report.json"
REPORT_JSON="$RUN_DIR/acceptance-report.json"
NODE_PID=""
PREALLOC_ADDRESS=""
SMOKE_STATUS="skipped"
EXTERNAL_BOOT_STATUS="skipped"
CELLSCRIPT_STATUS="skipped"
PROPAGATION_STATUS="skipped"
GIT_REVISION="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || true)"
GIT_STATUS_COUNT="$(git -C "$REPO_ROOT" status --porcelain=v1 2>/dev/null | wc -l | awk '{print $1}')"
CURRENT_STEP="setup"
RESULT="passed"
FAILURE_EXIT_CODE=""
FAILURE_LINE=""

mkdir -p "$RUN_DIR"

stop_node() {
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" >/dev/null 2>&1; then
    kill "$NODE_PID" >/dev/null 2>&1 || true
    wait "$NODE_PID" >/dev/null 2>&1 || true
  fi
  NODE_PID=""
}

cleanup() {
  stop_node
}
trap cleanup EXIT

run_smoke() {
  (
    cd "$REPO_ROOT"
    DEVNET_ACCEPTANCE_SMOKE_REPORT_JSON="$SMOKE_REPORT_JSON" \
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      devnet_acceptance_tests::devnet_acceptance_smoke \
      -- --nocapture --test-threads=1
  )
  validate_smoke_report
  mark_json_status "$SMOKE_REPORT_JSON" "passed"
  SMOKE_STATUS="passed"
}

validate_smoke_report() {
  python3 - "$SMOKE_REPORT_JSON" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)

expected_examples = [
    "amm_pool.cell",
    "launch.cell",
    "multisig.cell",
    "nft.cell",
    "timelock.cell",
    "token.cell",
    "vesting.cell",
]
examples = report.get("bundled_examples", [])
names = [item.get("name") for item in examples]
if names != expected_examples:
    raise SystemExit(f"unexpected bundled example deployment list: {names}")

required_true_fields = [
    "signed_transfer_confirmed",
    "multi_input_multi_output_confirmed",
    "parent_child_mempool_confirmed",
    "scheduler_tamper_rejected",
    "always_success_vm_spend_confirmed",
    "noop_cellscript_spend_confirmed",
    "cellscript_schema_output_spend_confirmed",
    "cellscript_parameterized_amount_spend_confirmed",
]
for field in required_true_fields:
    if report.get(field) is not True:
        raise SystemExit(f"smoke report field {field} was not true")

mass_policy = report.get("relaxed_mass_policy", {})
if mass_policy.get("relay_non_standard") is not True:
    raise SystemExit("smoke report did not record relaxed non-standard relay")
if mass_policy.get("block_max_mass") != 100000000:
    raise SystemExit(f"unexpected smoke block_max_mass: {mass_policy.get('block_max_mass')}")
if mass_policy.get("applies_to_all_networks_when_explicitly_enabled") is not True:
    raise SystemExit("smoke report did not record all-network explicit opt-in scope")
if mass_policy.get("standard_policy_preserved_by_default") is not True:
    raise SystemExit("smoke report did not record default standard policy preservation")

for item in examples:
    name = item.get("name")
    if not item.get("code_cell_indexed"):
        raise SystemExit(f"{name} code cell was not indexed according to smoke report")
    if not item.get("malformed_spend_rejected"):
        raise SystemExit(f"{name} malformed spend was not rejected according to smoke report")
    if item.get("malformed_spend_rejected_by_standard_policy"):
        raise SystemExit(f"{name} malformed spend was rejected by standard policy")
    reason = item.get("malformed_spend_reject_reason", "")
    lowered = reason.lower()
    forbidden = ["not standard", "storage mass", "compute mass", "transient", "cycles exceeded", "cycles limit"]
    if any(marker in lowered for marker in forbidden):
        raise SystemExit(f"{name} malformed spend did not fail fast in script/business validation: {reason}")
    if not item.get("artifact_size_bytes", 0) > 0:
        raise SystemExit(f"{name} artifact size was not recorded")
PY
}

mark_json_status() {
  local path="$1"
  local status="$2"
  python3 - "$path" "$status" <<'PY'
import json
import sys

path, status = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)
report["status"] = status
with open(path, "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2)
    fh.write("\n")
PY
}

run_cellscript_suite() {
  (
    cd "$REPO_ROOT"
    cargo test --locked -p cellscript --test examples -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_compiles_package_with_local_path_dependency -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_rejects_registry_package_dependencies_fail_closed -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_build_and_check_subcommands_use_package_flow -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_init_subcommand_supports_json_summary -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_info_subcommand_supports_json_summary -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_entry_witness_subcommand -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_add_and_remove_subcommands_honor_dev_path_and_json -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_install_path_updates_lockfile_and_remove_prunes_it -- --nocapture --test-threads=1
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      common::cellscript_contracts::tests::all_spora_examples_compile_metadata_acceptance \
      -- --nocapture --test-threads=1
  )
  CELLSCRIPT_STATUS="passed"
  cat > "$CELLSCRIPT_REPORT_JSON" <<JSON
{
  "profile": "cellscript",
  "status": "passed",
  "tests": [
    "cellscript_examples_action_metadata_suite",
    "cellc_compiles_package_with_local_path_dependency",
    "cellc_rejects_registry_package_dependencies_fail_closed",
    "cellc_build_and_check_subcommands_use_package_flow",
    "cellc_init_subcommand_supports_json_summary",
    "cellc_info_subcommand_supports_json_summary",
    "cellc_entry_witness_subcommand",
    "cellc_add_and_remove_subcommands_honor_dev_path_and_json",
    "cellc_install_path_updates_lockfile_and_remove_prunes_it",
    "all_spora_examples_compile_metadata_acceptance"
  ]
}
JSON
}

run_propagation_suite() {
  (
    cd "$REPO_ROOT"
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      daemon_integration_tests::daemon_cells_propagation_test \
      -- --nocapture --test-threads=1
  )
  PROPAGATION_STATUS="passed"
  cat > "$PROPAGATION_REPORT_JSON" <<JSON
{
  "profile": "propagation",
  "status": "passed",
  "tests": [
    "daemon_cells_propagation_test"
  ],
  "coverage": [
    "two-node block relay",
    "two-node transaction acceptance",
    "cellindex consistency",
    "address-scoped CellsChanged notifications"
  ]
}
JSON
}

bootstrap_wallet() {
  (
    cd "$REPO_ROOT"
    cargo run --locked -p spora-testing-integration --bin spora-devnet-bootstrap -- \
      --network devnet \
      --wallet-dir "$RUN_DIR/wallet" \
      --wallet-name acceptance \
      --prealloc-cells 101 \
      --prealloc-amount-sau 10000000000 \
      --out "$BOOTSTRAP_JSON" > "$RUN_DIR/bootstrap.stdout.json"
  )
}

json_value() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, dotted = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
for key in dotted.split("."):
    value = value[key]
print(value)
PY
}

run_external_boot() {
  bootstrap_wallet
  local address
  address="$(json_value "$BOOTSTRAP_JSON" "wallet.default_address")"
  PREALLOC_ADDRESS="$address"

  (
    cd "$REPO_ROOT"
    cargo run --locked --bin sporad --features devnet-prealloc -- \
      --devnet \
      --appdir "$RUN_DIR/sporad" \
      --num-prealloc-cells=101 \
      --prealloc-address="$address" \
      --prealloc-amount=10000000000 \
      --cellindex \
      --relaynonstd \
      --blockmaxmass=100000000 \
      --nodnsseed \
      --disable-upnp \
      --rpclisten=0.0.0.0:16610 \
      --rpclisten-borsh=0.0.0.0:17610 \
      --rpclisten-json=0.0.0.0:18610 \
      --enable-unsynced-mining \
      --skip-proof-of-work \
      --unsaferpc \
      --yes
  ) > "$SPORAD_LOG" 2>&1 &
  NODE_PID="$!"

  local started=0
  for _ in $(seq 1 600); do
    if ! kill -0 "$NODE_PID" >/dev/null 2>&1; then
      echo "sporad exited during external boot smoke; see $SPORAD_LOG" >&2
      tail -n 80 "$SPORAD_LOG" >&2 || true
      exit 1
    fi
    if grep -q "GRPC Server starting" "$SPORAD_LOG" >/dev/null 2>&1 && [[ "$(grep -c "WRPC Server starting" "$SPORAD_LOG" || true)" -ge 2 ]]; then
      started=1
      break
    fi
    sleep 1
  done
  if [[ "$started" -ne 1 ]]; then
    echo "sporad did not reach RPC startup during external boot smoke; see $SPORAD_LOG" >&2
    tail -n 80 "$SPORAD_LOG" >&2 || true
    exit 1
  fi

  (
    cd "$REPO_ROOT"
    cargo run --locked -p spora-testing-integration --bin spora-devnet-probe -- \
      --grpc grpc://127.0.0.1:16610 \
      --wrpc-borsh ws://127.0.0.1:17610 \
      --wrpc-json ws://127.0.0.1:18610 \
      --address "$address" \
      --expected-prealloc-cells 101 \
      --expected-prealloc-amount-sau 10000000000 \
      --out "$PROBE_JSON" > "$RUN_DIR/probe.stdout.json"
  )

  EXTERNAL_BOOT_STATUS="passed"
  stop_node
}

write_acceptance_report() {
  PROFILE="$PROFILE" \
  RUN_ID="$RUN_ID" \
  RUN_DIR="$RUN_DIR" \
  STARTED_AT_UTC="$STARTED_AT_UTC" \
  COMPLETED_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  ACCEPTANCE_COMMAND="$ACCEPTANCE_COMMAND" \
  GIT_REVISION="$GIT_REVISION" \
  GIT_STATUS_COUNT="$GIT_STATUS_COUNT" \
  BOOTSTRAP_JSON="$BOOTSTRAP_JSON" \
  PROBE_JSON="$PROBE_JSON" \
  SMOKE_REPORT_JSON="$SMOKE_REPORT_JSON" \
  CELLSCRIPT_REPORT_JSON="$CELLSCRIPT_REPORT_JSON" \
  PROPAGATION_REPORT_JSON="$PROPAGATION_REPORT_JSON" \
  SPORAD_LOG="$SPORAD_LOG" \
  PREALLOC_ADDRESS="$PREALLOC_ADDRESS" \
  SMOKE_STATUS="$SMOKE_STATUS" \
  EXTERNAL_BOOT_STATUS="$EXTERNAL_BOOT_STATUS" \
  CELLSCRIPT_STATUS="$CELLSCRIPT_STATUS" \
  PROPAGATION_STATUS="$PROPAGATION_STATUS" \
  RESULT="$RESULT" \
  CURRENT_STEP="$CURRENT_STEP" \
  FAILURE_EXIT_CODE="$FAILURE_EXIT_CODE" \
  FAILURE_LINE="$FAILURE_LINE" \
  python3 - "$REPORT_JSON" <<'PY'
import json
import os
import sys


def existing(env_name):
    path = os.environ.get(env_name, "")
    return path if path and os.path.exists(path) else None


def optional_int(env_name):
    value = os.environ.get(env_name, "")
    return int(value) if value else None


report = {
    "profile": os.environ["PROFILE"],
    "status": os.environ["RESULT"],
    "run_id": os.environ["RUN_ID"],
    "run_dir": os.environ["RUN_DIR"],
    "generated_at_utc": os.environ["STARTED_AT_UTC"],
    "completed_at_utc": os.environ["COMPLETED_AT_UTC"],
    "command": os.environ["ACCEPTANCE_COMMAND"],
    "git_revision": os.environ.get("GIT_REVISION") or None,
    "git_dirty": os.environ.get("GIT_STATUS_COUNT", "0") != "0",
    "bootstrap": existing("BOOTSTRAP_JSON"),
    "probe": existing("PROBE_JSON"),
    "smoke_report": existing("SMOKE_REPORT_JSON"),
    "cellscript_report": existing("CELLSCRIPT_REPORT_JSON"),
    "propagation_report": existing("PROPAGATION_REPORT_JSON"),
    "sporad_log": existing("SPORAD_LOG"),
    "prealloc_address": os.environ.get("PREALLOC_ADDRESS") or None,
    "smoke": os.environ["SMOKE_STATUS"],
    "external_boot": os.environ["EXTERNAL_BOOT_STATUS"],
    "cellscript": os.environ["CELLSCRIPT_STATUS"],
    "propagation": os.environ["PROPAGATION_STATUS"],
    "profiles": {
        "smoke": os.environ["SMOKE_STATUS"],
        "external_boot": os.environ["EXTERNAL_BOOT_STATUS"],
        "propagation": os.environ["PROPAGATION_STATUS"],
        "cellscript": os.environ["CELLSCRIPT_STATUS"],
    },
    "result": os.environ["RESULT"],
    "failed_step": os.environ["CURRENT_STEP"] if os.environ["RESULT"] != "passed" else None,
    "failure_exit_code": optional_int("FAILURE_EXIT_CODE"),
    "failure_line": optional_int("FAILURE_LINE"),
}

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2)
    fh.write("\n")
PY
}

on_error() {
  local exit_code="$?"
  local line_no="${1:-}"
  trap - ERR
  set +e
  RESULT="failed"
  FAILURE_EXIT_CODE="$exit_code"
  FAILURE_LINE="$line_no"
  write_acceptance_report
  echo "acceptance failed during step '$CURRENT_STEP' with exit code $exit_code; report: $REPORT_JSON" >&2
  exit "$exit_code"
}

trap 'on_error $LINENO' ERR

case "$PROFILE" in
  smoke)
    CURRENT_STEP="smoke"
    run_smoke
    ;;
  external-boot)
    CURRENT_STEP="external_boot"
    run_external_boot
    ;;
  cellscript)
    CURRENT_STEP="cellscript"
    run_cellscript_suite
    ;;
  propagation)
    CURRENT_STEP="propagation"
    run_propagation_suite
    ;;
  full)
    CURRENT_STEP="smoke"
    run_smoke
    CURRENT_STEP="external_boot"
    run_external_boot
    CURRENT_STEP="propagation"
    run_propagation_suite
    CURRENT_STEP="cellscript"
    run_cellscript_suite
    ;;
  *)
    echo "unsupported profile: $PROFILE" >&2
    exit 2
    ;;
esac

write_acceptance_report

if [[ "$KEEP_ARTIFACTS" -eq 1 ]]; then
  echo "acceptance artifacts: $RUN_DIR"
else
  echo "acceptance artifacts: $RUN_DIR"
fi
