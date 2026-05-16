# ZeroDrop

ZeroDrop 是基于 ZeroTier 私有 IP 的私有文件助手。当前代码处于 Phase 0/1：Tauri 桌面壳、React 前端、Rust Core、axum 本地 Agent Server。

## 启动

```bash
npm install
npm run tauri dev
```

首次启动会创建：

```text
~/.zerodrop/config.toml
```

## 验证本地 Agent

应用启动后，本机 HTTP Agent 默认监听：

```text
0.0.0.0:8765
```

本机验证：

```bash
curl --noproxy '*' http://127.0.0.1:8765/api/ping
```

ZeroTier 网络内的另一台机器验证：

```bash
curl --noproxy '*' http://PEER_ZEROTIER_IP:8765/api/ping
```

## 当前范围

- React 调用 Rust command。
- Rust 向 React 推送 `agent_status` 事件。
- `/api/ping` 返回本机节点信息。
- `Transport` trait 已建立，`HttpTransport` 可作为第一版实现入口，`QuicTransport` 仅占位。
