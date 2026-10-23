#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rust_test="crates/newpic-core/tests/settings.rs"
ui_test="apps/newpic/src/App.test.tsx"

required_rust_coverage=(
  'value.autostart' 'value.run_in_tray' 'value.save.enabled'
  'SaveDirectory::DownloadsNewPic' 'ImageFormat::Png'
  'value.naming.pattern' 'value.naming.minimum_width'
  'value.clipboard.enabled' 'value.open_editor_after_capture'
  'value.success_notifications' 'value.include_pointer' 'value.check_updates'
  'Size::new(800, 600)' 'value.saved_fixed_region, None'
  'schema_one_migrates_to_persistent_f9_shape'
  'include_str!("fixtures/settings-schema-1.json")'
  'value.hotkeys.region' 'value.hotkeys.repeat_region'
  'value.hotkeys.fixed_region' 'value.hotkeys.active_window' 'Language::Korean'
)
required_ui_coverage=(
  '로그인 시 자동 실행' '트레이에서 계속 실행' '캡처 완료 알림'
  '업데이트 자동 확인' '포인터 포함' 'F9 저장 영역 너비' 'F9 저장 영역 높이'
  '파일 자동 저장' '클립보드에 복사' 'Downloads/NewPic' '파일 형식'
  '영역 캡처 단축키' '마지막 영역 다시 캡처 단축키'
  '저장 영역 이동 캡처 단축키' '활성 창 캡처 단축키'
)

for needle in "${required_rust_coverage[@]}"; do
  grep -Fq "$needle" "$rust_test" || { echo "missing Rust default coverage: $needle" >&2; exit 1; }
done
for needle in "${required_ui_coverage[@]}"; do
  grep -Fq "$needle" "$ui_test" || { echo "missing UI default coverage: $needle" >&2; exit 1; }
done

rust_output="$(cargo test -p newpic-core --test settings default_preset_matches_every_approved_design_value -- --exact 2>&1)"
printf '%s\n' "$rust_output"
grep -Fq 'test default_preset_matches_every_approved_design_value ... ok' <<<"$rust_output"
grep -Fq '1 passed; 0 failed' <<<"$rust_output"

migration_output="$(cargo test -p newpic-core --test settings schema_one_migrates_to_persistent_f9_shape -- --exact 2>&1)"
printf '%s\n' "$migration_output"
grep -Fq 'test schema_one_migrates_to_persistent_f9_shape ... ok' <<<"$migration_output"
grep -Fq '1 passed; 0 failed' <<<"$migration_output"

ui_result="$(mktemp)"
trap 'rm -f "$ui_result"' EXIT
npm exec --workspace=@newpic/desktop -- vitest run src/App.test.tsx -t 'shows every approved default' --reporter=json --outputFile="$ui_result"
node - "$ui_result" <<'NODE'
const fs = require("node:fs");
const report = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
if (report.numPassedTests !== 1 || report.numFailedTests !== 0) {
  console.error(`expected exactly one passing UI default test, got total=${report.numTotalTests} passed=${report.numPassedTests} failed=${report.numFailedTests}`);
  process.exit(1);
}
NODE
