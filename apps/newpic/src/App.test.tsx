import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { Diagnostics, Settings } from "./api/newpic";

const settings: Settings = {
  schema_version: 2,
  autostart: true,
  run_in_tray: true,
  success_notifications: true,
  language: "korean",
  include_pointer: false,
  open_editor_after_capture: false,
  check_updates: false,
  save: { enabled: true, directory: "downloads_new_pic", format: "png", jpeg_quality: 90 },
  naming: { pattern: "%c", minimum_width: 3 },
  clipboard: { enabled: true },
  fixed_region: { width: 800, height: 600 },
  saved_fixed_region: null,
  hotkeys: { region: "F7", repeat_region: "F8", fixed_region: "F9", active_window: "F10" },
};

const diagnostics: Diagnostics = {
  os: "linux",
  session: "wayland",
  capture: {
    backend: "XdgDesktopPortal",
    capabilities: [{ operation: "region", state: "supported", reason: null, hint: null }],
  },
  hotkeys: [
    { action: "region", label: "영역 캡처", status: { state: "registered", accelerator: "F7" } },
  ],
  autostart: { state: "in_sync", enabled: true },
  destination: { path: "/home/user/Downloads/NewPic", state: "writable" },
  settings_notice: null,
  events: [],
};

const api = {
  getSettings: vi.fn(async () => structuredClone(settings)),
  saveSettings: vi.fn(async (value: Settings) => ({
    settings: value,
    hotkeys: diagnostics.hotkeys,
    rejected_shortcuts: [],
    autostart: diagnostics.autostart,
  })),
  applyDefaultPreset: vi.fn(async () => ({
    settings: structuredClone(settings),
    hotkeys: diagnostics.hotkeys,
    rejected_shortcuts: [],
    autostart: diagnostics.autostart,
  })),
  getDiagnostics: vi.fn(async () => diagnostics),
  openPermissionSettings: vi.fn(async () => false),
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

async function renderApp() {
  const user = userEvent.setup();
  render(<App api={api} />);
  await screen.findByRole("heading", { name: "일반" });
  return user;
}

describe("NewPic settings", () => {
  it("shows every approved default across the settings sections", async () => {
    const user = await renderApp();
    expect(screen.getByRole("checkbox", { name: "로그인 시 자동 실행" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "트레이에서 계속 실행" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "캡처 완료 알림" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "업데이트 자동 확인" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "캡처" }));
    expect(screen.getByRole("checkbox", { name: "포인터 포함" })).not.toBeChecked();
    expect(screen.getByLabelText("고정 영역 너비")).toHaveValue(800);
    expect(screen.getByLabelText("고정 영역 높이")).toHaveValue(600);
    await user.click(screen.getByRole("button", { name: "저장" }));
    expect(screen.getByRole("checkbox", { name: "파일 자동 저장" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "클립보드에 복사" })).toBeChecked();
    expect(screen.getByLabelText("저장 폴더")).toHaveValue("Downloads/NewPic");
    expect(screen.getByLabelText("파일 형식")).toHaveValue("png");
    await user.click(screen.getByRole("button", { name: "단축키" }));
    expect(screen.getByLabelText("영역 캡처 단축키")).toHaveValue("F7");
    expect(screen.getByLabelText("마지막 영역 다시 캡처 단축키")).toHaveValue("F8");
    expect(screen.getByLabelText("고정 크기 캡처 단축키")).toHaveValue("F9");
    expect(screen.getByLabelText("활성 창 캡처 단축키")).toHaveValue("F10");
  });

  it("shows the 001.png filename preview", async () => {
    const user = await renderApp();
    await user.click(screen.getByRole("button", { name: "파일 이름" }));
    expect(screen.getByText("001.png")).toBeVisible();
  });

  it("reveals JPEG quality only for JPEG", async () => {
    const user = await renderApp();
    await user.click(screen.getByRole("button", { name: "저장" }));
    const format = screen.getByLabelText("파일 형식");
    expect(screen.queryByLabelText("JPEG 품질")).not.toBeInTheDocument();
    await user.selectOptions(format, "jpeg");
    expect(screen.getByLabelText("JPEG 품질")).toHaveValue("90");
    await user.selectOptions(format, "png");
    expect(screen.queryByLabelText("JPEG 품질")).not.toBeInTheDocument();
  });

  it("associates shortcut conflicts with the affected input", async () => {
    const user = await renderApp();
    await user.click(screen.getByRole("button", { name: "단축키" }));
    const repeat = screen.getByLabelText("마지막 영역 다시 캡처 단축키");
    await user.clear(repeat);
    await user.type(repeat, "F7");
    const error = screen.getByText("영역 캡처와 같은 단축키입니다.");
    expect(repeat).toHaveAttribute("aria-describedby", error.id);
    expect(repeat).toHaveAttribute("aria-invalid", "true");
  });

  it("shows backend, permissions, hotkeys, autostart, and destination diagnostics", async () => {
    const user = await renderApp();
    await user.click(screen.getByRole("button", { name: "진단" }));
    await screen.findByText("XdgDesktopPortal");
    expect(screen.getByText("캡처 권한")).toBeVisible();
    expect(screen.getByText("전역 단축키")).toBeVisible();
    expect(screen.getByText("로그인 자동 실행")).toBeVisible();
    expect(screen.getByText("저장 위치")).toBeVisible();
    expect(screen.getByText("/home/user/Downloads/NewPic")).toBeVisible();
  });

  it("asks before resetting the preset without saying images are deleted", async () => {
    const user = await renderApp();
    await user.click(screen.getByRole("button", { name: "진단" }));
    await user.click(await screen.findByRole("button", { name: "기본 프리셋 다시 적용" }));
    const dialog = screen.getByRole("dialog", { name: "기본 프리셋 다시 적용" });
    expect(dialog).toHaveTextContent("캡처한 이미지는 그대로 유지됩니다.");
    expect(dialog).not.toHaveTextContent(/이미지.*삭제|삭제.*이미지/);
    expect(api.applyDefaultPreset).not.toHaveBeenCalled();
  });

  it("keeps every settings control in keyboard navigation order", async () => {
    const user = await renderApp();
    for (const name of ["일반", "캡처", "저장", "파일 이름", "단축키", "진단"]) {
      await user.tab();
      expect(screen.getByRole("button", { name })).toHaveFocus();
    }
    await user.tab();
    expect(screen.getByRole("checkbox", { name: "로그인 시 자동 실행" })).toHaveFocus();
  });
});
