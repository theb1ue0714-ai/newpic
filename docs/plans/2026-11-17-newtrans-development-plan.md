# NewTrans(가제) 개발계획서 — QTranslate 6.10.0 대체 크로스플랫폼 번역 도구

- 작성일: 2026-11-17
- 상태: **승인·구현 진행 중** (2026-11-17 사용자 결정 반영, 아래 '확정 사항'이 본문보다 우선)
- 배포 위치: GitHub 저장소 `theb1ue0714-ai/newtrans` → `https://newtil.com/newtrans`
- 코드 위치: `/data/newtrans`
- 참고 선례: NewPic(`/data/mysite/rea`, `newtil.com/newpic`)의 구조·CI·배포 방식

---


## 0. 확정 사항 (2026-11-17)

| 항목 | 결정 |
|---|---|
| 이름·배포 | NewTrans, `newtil.com/newtrans`에서 다운로드, NewPic처럼 GitHub Releases 연결 |
| 언어·구조 | Rust(Edition 2024) + Tauri 2, 새로 작성(포크 안 함) |
| 단축키 | 기본 Ctrl 두 번, 보조 Ctrl+Q. 사용자 지정은 수식키 두 번 누르기 또는 최대 3개 키 조합 |
| 번역 흐름 | ① 원문(언어 1, 자동 감지) → ② 번역(언어 2, 기본 한국어) → ③ 재번역(다시 언어 1). 원문이 이미 언어 2면 ②는 대체 언어(영어) |
| 언어 고정 | 번역 창에서 언어 1·2를 바꾸면 그 값이 기본값으로 저장·고정 |
| 기본 언어 | 한국어·영어·일본어·독일어·프랑스어·스페인어, 설정에서 14개 추가 가능 |
| 과금 | 과금 없는 방식 우선: Google은 키 없는 무료 엔드포인트 기본, DeepL은 API Free 키, Papago(NCP)는 유료라 선택 사항 |
| 키 | 배포본에 키 없음. 사용자는 직접 발급해 설정에 입력. 운영자 본인 키는 대리 발급 후 개인 PC 설정·서버 `.env`(테스트용)에만 저장 |
| AI 키 | 사용 안 함. 모든 처리는 로컬 앱에서 각 번역 서비스로 직접 요청 |
| 음성 읽기 | OS 내장 음성(Web Speech API)으로 간단히 제공 |
| 서명 | 1차는 미서명 개발 빌드 |
| 탭 전환 | Alt+Tab은 OS가 가로채므로 Ctrl+Tab·Alt+1~3·창이 활성일 때 단축키 재입력으로 대체 |
| 더블 Ctrl 구현 | 저수준 훅 대신 8ms 키 상태 폴링 + 상태 기계(Windows 11 훅 해제 문제 회피) |

## 1. 목표

텍스트를 선택한 뒤 단축키를 누르면 번역 창이 뜨고, Google·Papago·DeepL 번역 결과를 탭으로 바꿔 가며 볼 수 있는 Windows·macOS·Linux 공용 번역 도구를 만든다.

핵심 사용자 흐름(QTranslate에서 쓰던 방식 그대로):

1. 아무 프로그램에서 텍스트를 드래그한다.
2. **Ctrl 두 번 빠르게**(기본값) 또는 사용자 지정 단축키(예: `Ctrl+Shift+Z`)를 누른다.
3. 커서 근처에 번역 팝업이 뜨고, 기본 탭(Google)의 결과가 바로 보인다.
4. 탭 클릭 또는 키보드로 Papago·DeepL 탭으로 전환한다. 각 탭은 해당 서비스 결과를 보여준다.

## 2. 조사 결과 요약

### 2.1 원본 QTranslate 6.10.0 (QuestSoft)

| 항목 | 확인 내용 |
|---|---|
| 입수 파일 | `QTranslate.6.10.0.zip` SHA-256 `4f6393d8…4d4e19e` / 내부 `QTranslate.6.10.0.exe` SHA-256 `d395af3c…a013c` |
| 형식 | NSIS 3.08 설치 프로그램, 32비트 Win32 네이티브 앱(`QTranslate.exe`), BASS 오디오·libcurl·zlib 사용 |
| 구성 | 번역 서비스가 `Services/<이름>/Service.js` 스크립트 플러그인으로 분리됨(Google, DeepL, Naver(Papago), Microsoft, Yandex 등 20종), 언어팩 JSON 38종, 테마 JSON 8종 |
| 서비스 방식 | 공식 API 키가 아니라 **각 사이트의 웹 번역 엔드포인트를 흉내 내는 비공식 방식** → 사이트가 바뀌면 고장남. 원본이 점점 작동하지 않게 된 주요 원인 |
| 라이선스 | 프리웨어 EULA. **디컴파일·디스어셈블·수정 금지**, 수정 배포 금지 |

라이선스 때문에 원본은 **외부에서 관찰 가능한 동작과 사용자용 문서(도움말·설정 항목 이름)로 기능 목록을 파악하는 데에만** 사용했다. `QTranslate.exe`는 디스어셈블하지 않았고, 원본 코드·문구·아이콘·테마는 새 프로그램에 넣지 않는다. 원본 파일은 저장소 밖 작업 폴더에만 두고 커밋하지 않는다.

관찰한 기능 목록(도움말·설정 화면 기준):

- 전역 단축키: 더블 Ctrl(메인 창), Ctrl+Q(선택 텍스트 팝업 번역), Ctrl+Shift+Q(사전), Ctrl+E(음성 읽기). 단축키 동작 17종 지정 가능
- 팝업 창: 자동 크기·위치, 자동 숨김 지연, 투명도, 열 때 활성화, 팝업 머리글의 서비스 이름 클릭으로 서비스 전환
- 메인 창: Ctrl+Enter 번역, Ctrl+1~9 서비스 선택, Ctrl+I 번역 방향 반전, Alt+←/→ 기록 이동
- 마우스 모드: 선택 시 아이콘 표시 / 바로 번역 / 번역+읽기
- 일반: 윈도우 시작 시 실행, 첫째·둘째 언어(자동 방향 전환), 히스토리, 프록시, 예외 프로그램 목록, 줄바꿈 제거, 트레이 최소화

### 2.2 ahatem/QTranslate (GitHub 오픈소스)

| 항목 | 확인 내용 |
|---|---|
| 정체 | QuestSoft 원본과 **다른 프로그램**. 원본이 중단된 뒤 개인 개발자가 처음부터 새로 만든 후속작. 코드를 공유하지 않음 |
| 기술 | Kotlin/JVM + Swing UI, Gradle. 전역 입력은 자체 라이브러리 QInput(JNA 기반) |
| 플랫폼 | Windows는 Java 포함 ZIP, macOS·Linux는 **Java 17 별도 설치 필요** |
| 기능 | Ctrl+Q 빠른 번역 팝업, 더블 Ctrl(메인 창), 여러 서비스 비교 보기, OCR, TTS, 문서 번역, 플러그인(Google·DeepL·Bing·Yandex·LibreTranslate·AI 등). **Papago 플러그인은 없음** |
| 라이선스 | **MPL-2.0**(과거 커밋 `10cdad9a`까지는 MIT) |
| 최근 커밋 | `aec57a9b` (2026-11-16) — 활발히 유지보수 중 |

활용 방침:

- **설계 참고용으로만 쓴다.** 플러그인 경계, 단축키 설정 모델(더블 Ctrl을 일반 조합키와 분리한 구조), DeepL 키 접미사 `:fx`로 Free/Pro 엔드포인트를 고르는 방식, Google 공식 API와 비공식 API의 분리 같은 아이디어는 참고하되 코드는 Rust로 새로 작성한다.
- MPL-2.0은 **파일 단위 카피레프트**라서, 파일을 복사해 쓰면 그 파일은 MPL로 남고 소스 공개 의무가 생긴다. 언어도 달라 옮길 이점이 적으므로 **코드 복사는 하지 않는다**. 부득이 쓰게 되면 해당 파일 단위로 MPL 고지를 유지하고 `THIRD_PARTY_NOTICES.md`에 기록한다.
- 대안으로 ahatem 버전을 포크해 Papago 플러그인만 추가하는 방법도 있다(§10 결정 사항 D1).

### 2.3 Windows 11에서 더블 Ctrl이 안 되던 원인(추정)과 대응

더블 Ctrl은 일반 단축키 API(`RegisterHotKey`)로는 등록할 수 없어 **저수준 키보드 훅(`WH_KEYBOARD_LL`)** 으로 키 입력을 직접 감시해야 한다. 이 방식이 깨지는 대표적인 원인:

1. **훅 콜백 지연**: 콜백이 `LowLevelHooksTimeout`(Win11 기본 약 1초, 실제로는 더 짧게 적용되기도 함)을 넘기면 Windows가 훅을 조용히 해제한다. 32비트 구형 앱에서 흔하다.
2. **권한 격리(UIPI)**: 관리자 권한으로 실행된 창(작업 관리자, 일부 게임·보안 프로그램)이 앞에 있으면 일반 권한 프로그램의 훅에 키가 전달되지 않는다.
3. **원격 데스크톱·가상 키보드·일부 IME**가 Ctrl을 가상 키로 바꿔 보내는 경우.

대응:

- 훅은 **전용 스레드 + 즉시 반환 콜백**으로 만들고, 판정은 채널로 넘겨 별도 스레드에서 처리한다.
- **훅 감시(watchdog)**: 주기적으로 자기 진단 신호를 확인해 훅이 해제됐으면 다시 설치하고 진단 로그에 남긴다.
- 더블 Ctrl과 별개로 **`Ctrl+Shift+Z` 같은 일반 조합키를 항상 함께 등록**(기본 보조 단축키)해서, 훅이 막히는 환경에서도 번역이 가능하게 한다.
- 관리자 권한 창 위에서 안 되는 경우는 진단 화면에 원인을 표시하고, 선택적으로 "관리자 권한으로 실행" 옵션을 제공한다(기본값은 꺼짐).

## 3. 범위

### 3.1 1차 버전(MVP)에 포함

1. **전역 단축키**
   - 기본값: 더블 Ctrl(두 번 누르는 간격 기본 350ms, 설정 가능)
   - 보조 기본값: `Ctrl+Shift+Z`(Windows는 기본 켜짐, macOS는 `⌘⇧Z` 충돌 때문에 `⌃⇧Z`)
   - 사용자 지정: **최대 3개 키 조합**(수식키 0~2개 + 일반키 1개, 예: `Ctrl+Shift+Z`, `Alt+Q`) 또는 **수식키 더블탭**(Ctrl/Alt/Shift/⌘)
   - 단축키 녹화 UI, 다른 프로그램·OS 예약 키와 충돌 감지, 즉시 적용
2. **선택 텍스트 가져오기**(§5.3)
3. **번역 팝업 창**
   - 커서 근처 표시, 화면 밖으로 나가지 않게 자동 위치, 내용에 맞춘 자동 크기
   - 상단에 **Google / Papago / DeepL 탭**, 탭마다 독립적으로 번역 요청(동시 요청, 먼저 끝난 결과부터 표시)
   - 원문 표시·편집 후 재번역, 결과 복사, 언어 방향 반전, 핀 고정(자동 숨김 해제)
   - 포커스를 잃으면 자동 숨김(설정 가능), Esc로 닫기
4. **탭 전환 키보드 조작**: `Ctrl+Tab` / `Ctrl+Shift+Tab`, `Alt+1~3`, `←/→`(원문 입력란 밖에서) — §5.5 참고
5. **언어 설정**: 원본 자동 감지, 첫째 언어(기본 한국어)·둘째 언어(기본 영어). 원문이 첫째 언어면 둘째 언어로, 아니면 첫째 언어로 자동 번역
6. **API 키 관리**: 사용자가 설정 화면에서 자기 키를 입력, OS 보안 저장소에 저장, "키 검증" 버튼(§6)
7. **백그라운드 실행**: 트레이(메뉴 막대) 아이콘 상주, 창을 닫아도 종료하지 않음
8. **OS 시작 시 자동 실행**: 설치 직후 기본 켜짐
9. **설정 화면**: 단축키, 언어, 서비스 사용 여부·순서·기본 탭, API 키, 팝업 동작, 자동 실행, 진단
10. **진단**: 단축키 훅 상태, 접근성 권한(macOS), Wayland 제약, API 키 상태·남은 사용량(서비스가 제공하는 경우)
11. **배포**: Windows EXE·MSI, macOS DMG(Apple Silicon·Intel), Linux AppImage·DEB, `newtil.com/translate` 다운로드 페이지

### 3.2 1차 버전에서 제외(이후 별도 승인)

음성 읽기(TTS), 사전 창, 히스토리, OCR, 마우스 모드(선택 시 아이콘), 선택 텍스트를 번역문으로 바꾸기, 문서 번역, 추가 번역 서비스(Microsoft 등), 자동 업데이트, 다국어 UI(1차는 한국어·영어).

## 4. 기술 선택

NewPic과 동일한 스택을 쓴다. 이미 CI·서명 문서·릴리스 파이프라인·newtil 다운로드 구조가 검증돼 있어 재사용 이익이 크고, 전역 규칙(신규 데스크톱 도구는 Rust 우선)에도 맞는다.

| 영역 | 선택 | 이유 |
|---|---|---|
| 셸 | Tauri 2 | 단일 설치 파일, 작은 용량, 트레이·자동 실행·전역 단축키 플러그인 제공 |
| 백엔드 | Rust stable, Edition 2024 | 키 훅·클립보드·HTTP를 안정적으로 처리, 메모리 안전 |
| UI | TypeScript + React (NewPic과 동일 버전대) | 탭·설정 화면 구현, `DESIGN.md` 재사용 |
| HTTP | `reqwest`(rustls) + `tokio` | 서비스 3개 동시 요청, 타임아웃·취소 |
| 비밀 저장 | `keyring` 크레이트 | Windows 자격 증명 관리자 / macOS 키체인 / Linux Secret Service |
| 자동 실행 | `tauri-plugin-autostart` | OS별 표준 방식(레지스트리 Run, LaunchAgent, XDG autostart) |
| 일반 조합키 | `tauri-plugin-global-shortcut` | OS 표준 단축키 등록 |
| 더블탭 감지 | 플랫폼별 직접 구현(§5.2) | 수식키 단독 입력은 표준 API로 등록 불가 |

## 5. 아키텍처

### 5.1 워크스페이스 구성(신규 저장소)

```
newtrans/
├─ apps/newtrans/            Tauri 앱 (src: React UI, src-tauri: 앱 진입점·명령)
├─ crates/newtrans-core/     설정 모델, 단축키 문법·검증, 언어 방향 결정, 오류 타입 (플랫폼 무관)
├─ crates/newtrans-providers/ 번역 서비스 트레이트 + Google/Papago/DeepL 구현
├─ crates/newtrans-input/    단축키·더블탭 감지 공통 상태 기계 (플랫폼 무관, 단위 테스트 대상)
├─ crates/newtrans-windows/  저수준 키 훅, UI Automation 선택 텍스트, 클립보드
├─ crates/newtrans-macos/    CGEventTap, AX 선택 텍스트, 권한 확인
├─ crates/newtrans-linux/    X11 XInput2/PRIMARY 선택, Wayland 포털·제약 보고
└─ crates/newtrans-testing/  가짜 서비스 서버, 가짜 키 이벤트 드라이버
```

NewPic 저장소와는 분리한다(별도 GitHub 저장소·릴리스). NewPic의 CI 워크플로, 아이콘 생성 스크립트, 저장소 규칙 검사 스크립트, 서명 문서는 복사해 이름만 바꿔 쓴다(같은 소유자의 MIT 코드라 문제없음).

### 5.2 단축키 처리

`newtrans-input`의 상태 기계가 OS에서 받은 원시 키 이벤트(누름/뗌, 키 코드, 시각)를 입력받아 "트리거 발생"을 판정한다. 플랫폼 코드는 이벤트 전달만 맡는다.

- **더블탭 판정 규칙**: 수식키 단독 누름→뗌을 2회, 두 번째 누름이 첫 번째 뗌 후 `interval`(기본 350ms) 이내, 그 사이 다른 키 입력 없음, 각 누름이 길게 누르기(기본 300ms 초과)가 아님. Ctrl+C 같은 일반 사용 중에는 절대 발동하지 않아야 한다.
- **조합키 문법**: `Ctrl+Shift+Z`, `Alt+Q`, `DoubleTap(Ctrl)` 형식의 문자열을 파싱·정규화한다. 키는 최대 3개, 일반키는 최대 1개. Windows키·⌘ 단독, Alt+Tab·Ctrl+Alt+Del 같은 OS 예약 조합은 거부한다.
- **플랫폼별 이벤트 원천**

| OS | 일반 조합키 | 더블탭 |
|---|---|---|
| Windows 10/11 | `RegisterHotKey` | `WH_KEYBOARD_LL` 전용 스레드 + 감시·재설치 |
| macOS 13+ | Carbon `RegisterEventHotKey`(플러그인) | `CGEventTap`(listen-only). **손쉬운 사용·입력 모니터링 권한 필요** → 첫 실행 시 안내 |
| Linux X11 | `XGrabKey` | XInput2 raw 키 이벤트 |
| Linux Wayland | XDG GlobalShortcuts 포털(GNOME 48+/KDE 지원) | **불가**(보안 정책상 전역 키 감시 금지). 진단에 표시하고 조합키 사용 안내. 원하는 사용자만 `input` 그룹 + evdev 방식(옵트인, 보안 경고) |

### 5.3 선택 텍스트 가져오기

클립보드를 덮어쓰지 않는 방법을 먼저 쓰고, 안 될 때만 복사 시뮬레이션을 쓴다.

| OS | 1순위 | 2순위(대체) |
|---|---|---|
| Windows | UI Automation `TextPattern.GetSelection` | 클립보드 백업 → `Ctrl+C` 입력 전송 → 변경 감지(최대 300ms) → 읽은 뒤 **원래 클립보드 복원** |
| macOS | 접근성 API `AXSelectedText` | 위와 같은 방식(`⌘C`) |
| Linux X11 | PRIMARY 선택(드래그만 해도 채워짐) | 없음 |
| Linux Wayland | `wlr-data-control`/`ext-data-control` 프로토콜의 PRIMARY | `wl-paste --primary` |

- 더블 Ctrl 직후 Ctrl을 누른 상태로 `C`를 보내면 꼬일 수 있으므로, 시뮬레이션 전에 수식키가 모두 떼어졌는지 확인한다.
- 선택이 비어 있으면 빈 팝업(직접 입력 가능)을 띄운다.
- 클립보드·선택 텍스트는 로그에 남기지 않는다.

### 5.4 번역 서비스 계층

```rust
#[async_trait]
pub trait Translator: Send + Sync {
    fn id(&self) -> ProviderId;                       // Google | Papago | DeepL
    fn supports(&self, from: Lang, to: Lang) -> bool;
    async fn translate(&self, req: TranslateRequest, cancel: CancellationToken)
        -> Result<Translation, ProviderError>;
    async fn verify_key(&self) -> Result<KeyStatus, ProviderError>;
}
```

- 팝업이 열리면 활성 서비스 3개에 **동시 요청**, 각 요청 타임아웃 10초, 새 번역 요청 시 이전 요청 취소.
- `ProviderError`는 `MissingKey`, `InvalidKey`, `QuotaExceeded`, `UnsupportedLanguage`, `Network`, `Timeout`, `Upstream(code)`로 구분해 탭 안에 사람이 읽을 수 있는 안내를 표시한다(예: "DeepL 키가 없습니다 → 설정 열기").
- 같은 원문·언어쌍 결과는 메모리 캐시(최근 100건, 앱 종료 시 삭제)로 탭 왕복 시 재요청을 막는다.
- **공식 API만 사용한다.** 원본 QTranslate처럼 웹 번역 페이지를 흉내 내는 방식은 약관 위반 소지가 있고 금방 고장 나므로 쓰지 않는다.

### 5.5 팝업 UI와 탭 전환 키

- 탭 순서·기본 탭·사용 여부는 설정에서 바꿀 수 있다. 키가 없는 서비스 탭은 회색으로 표시하고 "키 등록" 안내를 띄운다.
- **`Alt+Tab`은 사용하지 않는다.** Windows·macOS·대부분의 Linux 데스크톱에서 OS가 창 전환용으로 먼저 가로채기 때문에 앱이 받을 수 없고, 억지로 가로채면 다른 프로그램 사용을 방해한다. 대신:
  - `Ctrl+Tab` / `Ctrl+Shift+Tab`: 다음/이전 탭 (브라우저와 같은 습관)
  - `Alt+1`, `Alt+2`, `Alt+3`: 해당 탭으로 바로 이동
  - 팝업에서 단축키(더블 Ctrl)를 한 번 더 누르면 다음 탭으로 순환 — 원본 QTranslate의 "서비스 이름 클릭으로 전환"에 대응
- 디자인은 NewPic의 `DESIGN.md` 토큰을 그대로 따르고, 다크/라이트 모드와 `prefers-reduced-motion`을 지원한다.

## 6. API 키 정책

### 6.1 서비스별 공식 API

| 서비스 | 공식 API | 키 발급처 | 무료 사용량(작성 시점 확인 필요) | 비고 |
|---|---|---|---|---|
| Google | Cloud Translation API v2(Basic) | Google Cloud 콘솔 → API 키 | 월 50만 자 무료 크레딧 | **결제 계정 등록 필수**. 키를 Translation API로만 제한 |
| Papago | 네이버 클라우드 플랫폼(NCP) Papago Translation | NCP 콘솔 → Application 등록 → Client ID/Secret | 유료(소량 무료 구간 여부 확인 필요) | 예전 developers.naver.com 파파고 API는 종료됨. 서버 `.env`의 기존 `NAVER_CLIENT_ID` 계열 키는 **Papago에 쓸 수 없을 가능성이 높음**(검증 예정) |
| DeepL | DeepL API Free / Pro | deepl.com 계정 → API 키 | Free: 월 50만 자 | Free 키는 `:fx`로 끝나며 `api-free.deepl.com` 사용 |

가격·무료 한도는 자주 바뀌므로 구현 직전에 각 공식 문서에서 다시 확인하고 설정 화면 도움말에 출처 링크를 둔다.

### 6.2 키를 어디에 두는가

- **배포되는 프로그램 안에는 어떤 API 키도 넣지 않는다.** 설치 파일에 넣은 키는 누구나 꺼낼 수 있고, 꺼낸 키로 사용량을 소진하거나 요금을 발생시킬 수 있다. 오픈소스로 공개하면 더욱 그렇다.
- 사용자는 설정 화면에서 자기 키를 입력하고, 키는 **OS 보안 저장소(keyring)** 에 저장된다. 설정 파일(JSON)에는 "키 있음/없음"만 기록한다.
- 개발·자동 테스트용 키는 서버의 `/data/mysite/.env`에 `NEWTRANS_GOOGLE_API_KEY`, `NEWTRANS_PAPAGO_CLIENT_ID`, `NEWTRANS_PAPAGO_CLIENT_SECRET`, `NEWTRANS_DEEPL_API_KEY` 이름으로 두고, 실제 API를 호출하는 통합 테스트(`--ignored`)에서만 읽는다. 저장소에는 `.env.example`(변수 이름만)만 커밋한다.
- 본인 PC에서는 첫 실행 후 설정 화면에 키를 한 번 붙여 넣으면 된다. 원하면 "환경변수에서 가져오기" 버튼으로 위 변수를 읽어 keyring에 저장하는 개발자 옵션을 둔다.

### 6.3 키 발급 진행 방식

- Google Cloud와 NCP는 **결제 수단 등록**이 필요하고, 키 발급은 요금이 발생할 수 있는 계정 설정 변경이다. 브라우저로 사용자 계정에 접속해 발급을 대신하는 작업은 **사용자 확인을 받은 뒤 별도 단계(Phase 5)** 로 진행한다. 직접 발급해서 전달해 주는 방법도 가능하다.
- 발급 시 Google 키는 Translation API 전용으로 제한하고, 사용량 알림(예산 경고)을 함께 설정한다.

## 7. 기본 설정(설치 직후 적용)

| 항목 | 기본값 |
|---|---|
| 주 단축키 | 더블 Ctrl (간격 350ms) |
| 보조 단축키 | Windows·Linux `Ctrl+Shift+Z`, macOS `⌃⇧Z` |
| OS 시작 시 자동 실행 | 켜짐 |
| 시작 시 창 | 표시하지 않음(트레이 상주) |
| 창 닫기 | 트레이로 숨김(종료 아님) |
| 탭 순서 / 기본 탭 | Google → Papago → DeepL / Google |
| 첫째·둘째 언어 | 한국어 · 영어 (자동 방향 전환) |
| 팝업 자동 숨김 | 포커스를 잃으면 숨김, 핀 고정 시 유지 |
| 줄바꿈 정리 | 켜짐(PDF 등에서 복사한 줄바꿈을 문장으로 합침) |
| 원격 전송 | 번역 요청 외 통신 없음(텔레메트리 없음) |

설정 파일 위치: Windows `%APPDATA%\NewTrans\settings.json`, macOS `~/Library/Application Support/NewTrans/`, Linux `~/.config/newtrans/`. 스키마 버전 필드를 두고 이전 버전 설정을 자동 변환한다.

## 8. 개발 단계

각 작업은 테스트를 먼저 작성하고(RED 확인) 구현해 통과(GREEN)시키는 순서로 진행한다.

### Phase 0 — 저장소 준비 (0.5일)
- 이름·도메인 확인 후 `theb1ue0714-ai/newtrans` 저장소 생성, Cargo/npm 워크스페이스, `rust-toolchain.toml`, `AGENTS.md`, 클린룸 정책, `DESIGN.md`, CI(ubuntu·windows·macos) 구성
- 완료 기준: 빈 Tauri 앱이 3개 OS CI에서 빌드되고 품질 게이트 통과

### Phase 1 — 코어와 번역 서비스 (2일)
- 설정 모델·검증·마이그레이션, 언어 자동 방향 결정, 단축키 문법 파서
- `Translator` 트레이트, Google·Papago·DeepL 구현, 오류 분류, 동시 요청·취소·캐시
- 테스트: 가짜 HTTP 서버로 정상/401/403/429/456(DeepL 한도)/타임아웃 응답 검증, 실제 키 통합 테스트(`cargo test -- --ignored`, 서버 `.env` 사용)
- 완료 기준: CLI 테스트 명령 `newtrans-cli translate --provider deepl "Hello"`가 3개 서비스 모두에서 결과 출력

### Phase 2 — 입력 계층 (3일)
- 플랫폼 무관 더블탭 상태 기계 + 퍼즈/표 기반 테스트(Ctrl+C 연타, 길게 누르기, 다른 키 끼어들기 등 오발동 사례)
- Windows 저수준 훅·감시·재설치, macOS 이벤트 탭·권한 확인, X11 XInput2, Wayland 포털·제약 보고
- 선택 텍스트 가져오기(OS별 1·2순위)와 클립보드 복원
- 완료 기준: 각 OS에서 메모장·브라우저·PDF 뷰어·워드에서 선택→더블 Ctrl→텍스트 획득 수동 확인표 통과

### Phase 3 — 팝업·설정 UI (2.5일)
- 번역 팝업(탭, 로딩·오류 상태, 복사, 방향 반전, 핀), 커서 근처 배치·다중 모니터·고DPI
- 설정 화면(단축키 녹화·충돌 경고, 언어, 서비스 순서, API 키 입력·검증, 자동 실행, 진단)
- 트레이 메뉴(번역 창 열기, 설정, 일시 중지, 종료)
- 완료 기준: 키보드만으로 모든 기능 조작 가능, 스크린리더 레이블 확인

### Phase 4 — 패키징·배포 (1.5일)
- 설치 파일 빌드(EXE·MSI·DMG×2·AppImage·DEB), `v*` 태그 시 GitHub Release 자동 게시
- newtil 연동: `includes/translate-release.php`, `translate-download.php`, `translate.php`, `.htaccess`에 `^translate/?$`, `^translate/download/(…)/?$` 규칙 추가 — `newpic.php`·`newpic-download.php`·`includes/newpic-release.php` 구조를 그대로 따름(설치 파일은 서버에 두지 않고 GitHub Release로 연결)
- Playwright로 다운로드 페이지 레이아웃·드롭다운 겹침 확인, 6개 다운로드 링크 HTTP 200 확인
- 서명: 1차는 NewPic과 같이 "미서명 개발 빌드"로 표시. Windows SmartScreen·macOS Gatekeeper 경고 우회 안내 포함. 정식 배포 시 서명 인증서 필요(§10 D4)

### Phase 5 — API 키 발급·실사용 검증 (0.5일, 사용자 승인 후)
- 사용자 계정으로 각 콘솔에서 키 발급 또는 사용자가 직접 전달 → 서버 `.env` 등록 → 통합 테스트 → 본인 PC 설정에 입력
- 1주일 실사용 후 피드백 반영(v0.1.1)

**예상 총 소요: 약 10일(작업일 기준).** Windows·macOS 실기기 확인이 필요한 단계는 사용자 PC 테스트 일정에 따라 늘어날 수 있다.

## 9. 품질·보안 기준

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `npm test`, `npm run typecheck`, `cargo deny check`, `npm audit`
- `unsafe`는 OS API 호출 래퍼(훅·접근성)에만 허용하고, 각 블록에 안전 조건을 주석으로 남긴다
- API 키·선택 텍스트·번역 원문을 로그·오류 메시지·크래시 리포트에 남기지 않는다(로그에는 서비스 이름·상태 코드·소요 시간만)
- 저장소 커밋 전 비밀값 검사(gitleaks 등) 실행
- HTTPS만 사용, 프록시는 OS 설정 따름
- 원본 QTranslate 파일·문구·아이콘, ahatem 코드는 저장소에 넣지 않는다(§2)

## 10. 사용자 결정이 필요한 사항

| 번호 | 내용 | 제안 |
|---|---|---|
| D1 | 새로 만들기(Rust/Tauri) vs ahatem/QTranslate 포크 후 Papago 추가 | **새로 만들기**. 포크는 빠르지만 macOS·Linux에서 Java 설치가 필요하고, Swing UI·MPL 라이선스·남의 일정에 묶인다. 새로 만들면 NewPic과 같은 방식으로 설치·배포·유지보수할 수 있다 |
| D2 | 프로그램 이름 | `NewTrans`(가제). NewPic과 짝을 맞춤. 상표·도메인 확인 후 확정 |
| D3 | 탭 전환 키 | `Alt+Tab` 대신 `Ctrl+Tab`·`Alt+1~3`·단축키 재입력 순환(§5.5) |
| D4 | 코드 서명 | 1차는 미서명 개발 빌드. Windows 인증서(연 수십만 원)·Apple 개발자 계정(연 $99)은 정식 공개 시 결정 |
| D5 | API 키 발급 방식 | 직접 발급해 전달 / 브라우저로 대리 발급(결제 수단 등록 단계는 사용자가 직접 확인) 중 선택 |
| D6 | 1차 범위 | §3.1 그대로 진행, 음성 읽기·히스토리는 2차 |
| D7 | 저장소 공개 여부·라이선스 | 처음엔 비공개로 개발, 안정화 후 MIT로 공개 제안 |

## 11. 위험 요소

| 위험 | 영향 | 대응 |
|---|---|---|
| Wayland에서 더블 Ctrl 불가 | Ubuntu 기본 세션 사용자 | 포털 조합키 기본 사용, 진단 안내, 옵트인 evdev |
| macOS 권한 거부 | 더블탭·선택 텍스트 불가 | 첫 실행 안내 화면, 권한 없을 때 조합키·클립보드 번역으로 동작 |
| 보안 프로그램이 키 훅을 악성으로 오탐 | 설치 차단 | 코드 서명, 훅은 listen-only, 입력 내용 저장 안 함을 문서화 |
| 서비스 요금·정책 변경 | 번역 실패·과금 | 공식 API만 사용, 오류 분류·안내, 서비스별 사용 끄기 |
| Papago(NCP) 유료 | 사용자 부담 | 무료 구간 확인, 키 없으면 탭 비활성 안내 |
| 선택 텍스트 복사 시뮬레이션이 원래 클립보드를 망침 | 데이터 손실 | 백업·복원, 이미지·서식 클립보드도 보존, 테스트로 검증 |

## 부록 A. 참고 자료

- QTranslate 6.10.0 설치 파일(사용자 제공, 저장소 밖 보관) — 도움말·설정 항목 관찰용
- ahatem/QTranslate `aec57a9b`(MPL-2.0) — 설계 참고용, 코드 미사용
- NewPic 저장소(`/data/mysite/rea`) — 스택·CI·배포 구조 재사용
- newtil 다운로드 구조: `/data/mysite/newpic.php`, `/data/mysite/newpic-download.php`, `/data/mysite/includes/newpic-release.php`, `/data/mysite/.htaccess` 336–337행
