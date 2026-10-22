# NewPic Task 1–7 Implementation Handoff Report

**작성일:** 2026-10-22  
**프로젝트:** NewPic  
**저장소 루트:** `/data/mysite/rea`  
**개발 worktree:** `/data/mysite/rea/.worktrees/newpic-mvp`  
**개발 브랜치:** `feature/newpic-mvp`  
**보고서 작성 전 HEAD:** `a09da80`  
**승인된 설계:** `docs/superpowers/specs/2026-10-22-newpic-design.md`  
**구현 계획:** `docs/plans/2026-10-22-newpic-mvp-implementation.md`

## 1. 현재 상태 요약

Task 1부터 Task 7까지 구현과 작업별 독립 코드 리뷰를 완료했다. Task 8은 구현을 시작하지 않았으며, 다음 에이전트가 TDD를 이어갈 수 있도록 출력 파이프라인의 RED 테스트만 커밋했다.

- 완료: Task 1–7
- RED 상태로 인계: Task 8 테스트 명세
- 미착수: Task 8 구현 및 Task 9–12
- Linux 비대화형 빌드·단위 테스트: 완료
- Windows/macOS 네이티브 컴파일·실행: 미검증
- Linux GUI 대화형 캡처: 미검증
- 공개 배포 서명·공증: 미착수

현재 `cargo test --workspace`는 Task 8의 의도적인 RED 테스트 때문에 실패한다. 실패 원인은 `newpic_core::image_output`, `newpic_core::pipeline`, `newpic_platform::clipboard`이 아직 구현되지 않았기 때문이다. 이 상태는 회귀가 아니라 다음 TDD 작업의 시작점이다.

## 2. Git 상태

### 브랜치

- 기준 문서 브랜치: `main`
- 구현 브랜치: `feature/newpic-mvp`
- 구현 worktree: `/data/mysite/rea/.worktrees/newpic-mvp`

### 주요 커밋

- `302d332` — REA 설치 환경과 클린룸 조사 규칙
- `eb3e4a8`, `5c5d56f` — 저장소 규칙, 디자인 시스템, 제3자 라이선스
- `b4fc411`, `ebc9c36` — Tauri/Rust/React 스캐폴딩과 실제 프런트엔드 테스트
- `68977d7` ~ `90794b6` — 설정 프리셋, 마이그레이션, 원자 저장, 프로세스 간 경쟁 안전성
- `591a719` ~ `0286100` — 파일명, 무덮어쓰기 저장, 복구, Windows 경로 안전성
- `dd8a412` ~ `1a6c15e` — 캡처 계약, DPI/토폴로지, F8 저장 영역, 테스트 Fake
- `09d8410` ~ `9789a03` — Linux/Windows/macOS 네이티브 캡처 어댑터
- `a09da80` — Task 8 출력 파이프라인 RED 테스트

### 원격 저장소

보고서 작성 시점에 `git remote -v` 결과가 비어 있다. 따라서 외부 Git 서버로의 `push`는 수행할 수 없다. 원격 URL과 인증을 준비한 후 다음 명령으로 푸시한다.

```bash
cd /data/mysite/rea
git remote add origin <REMOTE_GIT_URL>
git push -u origin main
git push -u origin feature/newpic-mvp
```

원격을 추가하기 전까지는 로컬 Git 커밋과 worktree가 유일한 소스이므로 `/data/mysite/rea` 전체를 보존해야 한다.

## 3. Task별 구현 내용

### Task 1 — REA 전역 설치와 검증

- `rea-agents@6.7.0` 전역 설치
- `rea setup --client codex --yes --json` 적용
- `rea doctor --client codex --json` 정상
- Node.js `24.15.0`, npm `11.12.1`, Rust `1.99.0`
- 연구 증거 형식과 클린룸 금지 규칙 작성
- Hopper/Ghidra/IDA는 설치하지 않음

새 에이전트 세션에서는 REA MCP가 노출될 수 있다. 확인 명령:

```bash
rea --version
rea doctor --client codex --json
```

### Task 2 — 저장소·디자인·라이선스 규칙

- `/home/mint/AGENT.md`를 참조하는 `AGENTS.md`
- 실제 Raycast `DESIGN.md`를 보존하고 NewPic 설정 앱 기준으로 분리 적용
- 전체 MIT 제3자 고지: `THIRD_PARTY_NOTICES.md`
- 클린룸 정책: `docs/architecture/clean-room-policy.md`
- 저장소 계약 검사: `scripts/check-repo-contract.sh`
- Node/npm 워크스페이스 기준을 README에 명시

### Task 3 — Tauri/Rust/React 스캐폴딩

- Tauri 2.12 계열
- React/Vite/TypeScript npm workspace
- Rust workspace 및 6개 crate: core, platform, Windows, macOS, Linux, testing
- Rust toolchain `1.99.0` 고정
- 제한적인 Tauri CSP
- 실제 React 렌더 테스트
- Linux `tauri build --no-bundle` 성공

### Task 4 — 설정과 승인 프리셋

구현된 기본값:

- 로그인 자동실행·트레이·자동저장·클립보드 저장 켜짐
- `Downloads/NewPic`, PNG, `%c`, 최소 3자리
- 편집기 자동 열기·포인터 포함·업데이트 자동 확인 꺼짐
- 고정 영역 800×600
- F7/F8/F9/F10

추가 구현:

- schema-0 → schema-1 마이그레이션
- 미래 스키마 비파괴 처리
- 손상 JSON 백업과 기본값 복구
- 원자 저장과 `fs2` OS 파일 잠금
- 초기화·마이그레이션·복구의 CAS 처리
- 동시 프로세스가 쓴 최신 설정을 덮어쓰지 않음
- 관련 테스트 23개

### Task 5 — 파일명과 원자 이미지 저장

- `%c`, 날짜·시간·호스트 토큰
- `001.png`, `999.png` → `1000.png`
- 빈 번호 재사용
- 최종 게시를 배타 할당점으로 사용
- 게시 충돌 시 기본 폴더에서 다음 번호 재시도
- 고정명 충돌 시 typed `NameUnavailable`
- 저장 불가 때에만 복구 폴더 사용
- 임시 형제 파일, flush, `sync_all`, 무덮어쓰기 게시
- Windows 예약 이름·금지 문자·ADS·장치 경로·후행 점/공백 차단
- `COM¹–³`, `LPT¹–³`까지 차단
- 스레드와 subprocess 동시 게시 테스트
- 관련 핵심 테스트 46개

### Task 6 — 캡처 계약과 좌표/DPI 모델

- `CaptureProvider`와 명시적 `CaptureOptions`
- `include_pointer`
- `CapturedImage` RGBA/stride 불변식
- `Uniform`/`Mixed`/`Gap` 배율 메타데이터
- 중첩 우선순위: 낮은 provider z-order, 동률이면 Display ID
- 음수 원점과 로컬 반올림
- F8 `SavedRegion`에 전체 토폴로지와 논리 교차 영역 저장
- 동일 토폴로지에서 원래 영역 정확히 재사용
- 안전하지 않은 DPI/방향/참여 화면 변경은 F7 폴백
- 포털 소유 선택 provenance
- 스크립트 가능한 thread-safe Fake Provider
- 관련 집중 테스트 37개

### Task 7 — 플랫폼 캡처 어댑터

#### Linux

- X11: `xcap 0.9.8`
- Wayland: `ashpd` XDG Screenshot Portal
- Wayland 영역 캡처는 compositor가 선택을 소유하는 제한 모드
- 포털 취소/권한 거부를 ashpd 타입으로 매핑
- 추정 좌표로 포털 PNG를 자르는 동작 제거
- Wayland `displays()`는 가짜 좌표 대신 typed `PortalOwnedSelection`
- 포인터 포함 여부는 compositor-controlled

#### Windows

- Windows Graphics Capture 우선
- Win32 GDI 런타임 폴백
- 실제 백엔드와 WGC 실패 이유를 provenance에 기록
- GDI `SelectObject` 검증, 비트맵 복원 후 `GetDIBits`
- RAII 자원 정리
- PID/Window ID로 NewPic 창 제외

#### macOS

- ScreenCaptureKit
- macOS 13.1 이상: `SCWindow.isActive` 우선
- macOS 13.0: frontmost PID + CoreGraphics 전면 창 순서
- 화면 기록 권한 오류와 설정 딥링크
- BGRA → RGBA 변환과 포인터 옵션

#### 공통 제한

- Windows/macOS 코드는 Linux에서 target compile/runtime 실행하지 못했다.
- 그래픽 세션이 없어 Linux interactive capture 테스트도 실행하지 못했다.
- 각 OS CI와 실제 권한 환경에서 반드시 재검증해야 한다.

## 4. 설치한 시스템 빌드 의존성

사용자 승인 후 Ubuntu 전역에 설치했다.

```text
libwebkit2gtk-4.1-dev
build-essential
curl
wget
file
libxdo-dev
libssl-dev
libayatana-appindicator3-dev
librsvg2-dev
libdbus-1-dev
libpipewire-0.3-dev
libclang-dev
libgbm-dev
```

확인된 주요 버전:

- GTK 3.24.41
- WebKitGTK 4.1 / 2.52.6
- DBus 1.14.10
- PipeWire 1.0.5
- SPA 0.2
- libclang 18
- GBM 25.2.8

APT 설치 과정에서 관련 Ubuntu 보안·업데이트 패키지도 함께 갱신됐다.

## 5. 마지막 정상 검증 범위

Task 7 완료 시 다음 검사가 통과했다.

```bash
cargo test --workspace
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm test -- --run
npm run typecheck
npm run build --workspace=@newpic/desktop
npm run build -- --no-bundle
bash scripts/check-repo-contract.sh
git diff --check
```

현재 HEAD에는 Task 8 RED 테스트가 추가됐으므로 `cargo test --workspace`는 의도적으로 실패한다. Task 8 구현 후 전체 게이트를 다시 실행해야 한다.

## 6. Task 8 RED 인계 상태

파일:

```text
crates/newpic-core/tests/pipeline.rs
```

현재 테스트가 요구하는 API:

- `newpic_core::image_output`: `encode_image`, `ImageEncoding`, `JpegQuality`
- `newpic_core::pipeline`: `CapturePipeline`, `DeliveryError`, `ImageStore`, `ImageStoreError`, `StoredOutput`
- `newpic_platform::clipboard`: `ClipboardError`, `ClipboardErrorKind`, `ClipboardImage`, `ImageClipboard`

테스트 범위:

- 저장+클립보드 성공
- 한쪽 실패의 부분 성공
- 양쪽 실패 집계
- clipboard busy 제한 재시도
- 영구 오류 무재시도
- PNG 픽셀 무결성
- padded stride 제거와 캡처 버퍼 불변
- JPEG 품질 경계
- 복구 저장 결과 전달

RED 확인:

```bash
cd /data/mysite/rea/.worktrees/newpic-mvp
cargo test -p newpic-core --test pipeline
```

예상 결과: 위 모듈/API가 없어 컴파일 실패.

## 7. 남은 작업

### Task 8

출력 인코더, `ImageStore`/`ImageClipboard` 포트, `arboard` 어댑터, 부분 성공 보고서, 제한적 clipboard retry.

### Task 9

Tauri state/commands, 트레이 메뉴, F7/F8/F9/F10, 자동실행, 진단.

### Task 10

다중 모니터 선택 overlay, 영역·고정 크기 reducer, ESC 취소, 동시 캡처 직렬화. Wayland는 portal-owned selection으로 분기.

### Task 11

일반/캡처/저장/파일명/단축키/진단 설정 UI, 접근성, 키보드 탐색, 다크 모드.

### Task 12

3개 OS CI, 설치 패키지 workflow, 수동 release matrix, GUI smoke test, 최종 코드 리뷰.

## 8. 다음 에이전트 권장 시작 순서

1. 이 보고서, `AGENTS.md`, 승인 설계와 계획을 읽는다.
2. `feature/newpic-mvp`와 보고서 커밋을 확인한다.
3. `rea doctor --client codex --json`을 실행한다.
4. Task 8 RED 테스트가 예상 이유로 실패하는지 확인한다.
5. Task 8을 테스트 우선으로 구현한다.
6. Task별 코드 리뷰 후 다음 Task로 이동한다.
7. Windows/macOS CI 전에는 해당 플랫폼 완료를 주장하지 않는다.

권장 명령:

```bash
cd /data/mysite/rea/.worktrees/newpic-mvp
git status --short
git branch --show-current
git log -5 --oneline
rea doctor --client codex --json
cargo test -p newpic-core --test pipeline
```

## 9. 위험 및 주의사항

- Wayland 포털이 영역 선택을 소유하므로 Windows/X11과 동일한 F8 반복 좌표를 보장하지 않는다.
- Windows/macOS 네이티브 코드는 해당 OS에서 아직 컴파일되지 않았다.
- ScreenCaptureKit/WGC/GDI는 실제 권한·다중 모니터·혼합 DPI 환경에서 검증해야 한다.
- 공개 MSI/DMG 배포에는 코드 서명과 macOS 공증 자격 증명이 필요하다.
- PicPick 코드·아이콘·문구·브랜드 자산은 포함하지 않는다.
- `research/`에는 합법적으로 취득한 대상의 해시·출처·관찰 결과만 기록한다.
- Task 8 RED 상태에서 전체 workspace 테스트 실패는 의도된 상태다.

