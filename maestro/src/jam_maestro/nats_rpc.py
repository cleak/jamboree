"""Minimal NATS client for Maestro tool calls and wake events."""

from __future__ import annotations

import asyncio
import json
import secrets
from dataclasses import dataclass
from typing import TYPE_CHECKING, cast
from urllib.parse import urlparse

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

MIN_MSG_PARTS = 4
MIN_HMSG_PARTS = 5


class NatsRpcError(RuntimeError):
    """Raised when a NATS request cannot complete cleanly."""


@dataclass(frozen=True)
class NatsJsonMessage:
    """One JSON object received from NATS."""

    subject: str
    headers: dict[str, str]
    payload: dict[str, object]


async def request_json(
    *,
    nats_url: str,
    subject: str,
    payload: dict[str, object],
    trace_id: str,
    timeout_secs: float = 5.0,
) -> dict[str, object]:
    """Send a traced JSON request and return a JSON object response."""
    host, port = _parse_nats_url(nats_url)
    reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout_secs)
    try:
        await _read_info(reader, timeout_secs)
        connect = {
            "lang": "jam-maestro",
            "version": "0.1.0",
            "protocol": 1,
            "headers": True,
        }
        writer.write(f"CONNECT {json.dumps(connect, separators=(',', ':'))}\r\n".encode())

        inbox = f"_INBOX.JAM.{secrets.token_hex(12)}"
        writer.write(f"SUB {inbox} 1\r\n".encode())
        _write_hpub(
            writer,
            _Hpub(subject=subject, reply_to=inbox, payload=payload, trace_id=trace_id),
        )
        await writer.drain()
        return await _read_json_message(reader, writer, timeout_secs)
    finally:
        writer.close()
        await writer.wait_closed()


async def publish_json(
    *,
    nats_url: str,
    subject: str,
    payload: dict[str, object],
    trace_id: str,
    parent_trace_id: str | None = None,
    timeout_secs: float = 5.0,
) -> None:
    """Publish one traced JSON object without waiting for a reply."""
    host, port = _parse_nats_url(nats_url)
    reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout_secs)
    try:
        await _read_info(reader, timeout_secs)
        connect = {
            "lang": "jam-maestro",
            "version": "0.1.0",
            "protocol": 1,
            "headers": True,
        }
        writer.write(f"CONNECT {json.dumps(connect, separators=(',', ':'))}\r\n".encode())
        _write_hpub(
            writer,
            _Hpub(
                subject=subject,
                reply_to=None,
                payload=payload,
                trace_id=trace_id,
                parent_trace_id=parent_trace_id,
            ),
        )
        await writer.drain()
    finally:
        writer.close()
        await writer.wait_closed()


async def next_json_message(
    *,
    nats_url: str,
    subject: str,
    timeout_secs: float = 30.0,
) -> NatsJsonMessage:
    """Subscribe to `subject` and return the next JSON object message."""
    host, port = _parse_nats_url(nats_url)
    reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout_secs)
    try:
        await _read_info(reader, timeout_secs)
        connect = {
            "lang": "jam-maestro",
            "version": "0.1.0",
            "protocol": 1,
            "headers": True,
        }
        writer.write(f"CONNECT {json.dumps(connect, separators=(',', ':'))}\r\n".encode())
        writer.write(f"SUB {subject} 1\r\n".encode())
        await writer.drain()
        return await _read_json_nats_message(reader, writer, timeout_secs)
    finally:
        writer.close()
        await writer.wait_closed()


async def subscribe_json(
    *,
    nats_url: str,
    subject: str,
    timeout_secs: float = 30.0,
) -> AsyncIterator[NatsJsonMessage]:
    """Yield JSON object messages from a core NATS subscription."""
    host, port = _parse_nats_url(nats_url)
    reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout_secs)
    try:
        await _read_info(reader, timeout_secs)
        connect = {
            "lang": "jam-maestro",
            "version": "0.1.0",
            "protocol": 1,
            "headers": True,
        }
        writer.write(f"CONNECT {json.dumps(connect, separators=(',', ':'))}\r\n".encode())
        writer.write(f"SUB {subject} 1\r\n".encode())
        await writer.drain()
        while True:
            try:
                yield await _read_json_nats_message(reader, writer, timeout_secs)
            except TimeoutError:
                continue
    finally:
        writer.close()
        await writer.wait_closed()


@dataclass(frozen=True)
class _Hpub:
    subject: str
    reply_to: str | None
    payload: dict[str, object]
    trace_id: str
    parent_trace_id: str | None = None


def _parse_nats_url(nats_url: str) -> tuple[str, int]:
    parsed = urlparse(nats_url)
    if parsed.scheme != "nats" or parsed.hostname is None:
        message = f"unsupported NATS URL: {nats_url}"
        raise NatsRpcError(message)
    if parsed.username or parsed.password:
        message = "NATS URL userinfo is unsupported; use token env once auth lands"
        raise NatsRpcError(message)
    return parsed.hostname, parsed.port or 4222


async def _read_info(reader: asyncio.StreamReader, timeout_secs: float) -> None:
    line = await asyncio.wait_for(reader.readline(), timeout_secs)
    if not line.startswith(b"INFO "):
        message = f"expected NATS INFO line, got {line!r}"
        raise NatsRpcError(message)


def _write_hpub(writer: asyncio.StreamWriter, message: _Hpub) -> None:
    parent = f"Parent-Trace-Id: {message.parent_trace_id}\r\n" if message.parent_trace_id else ""
    headers = f"NATS/1.0\r\nTrace-Id: {message.trace_id}\r\n{parent}\r\n".encode()
    body = json.dumps(message.payload, separators=(",", ":")).encode()
    total_len = len(headers) + len(body)
    if message.reply_to:
        writer.write(
            f"HPUB {message.subject} {message.reply_to} {len(headers)} {total_len}\r\n".encode()
        )
    else:
        writer.write(f"HPUB {message.subject} {len(headers)} {total_len}\r\n".encode())
    writer.write(headers)
    writer.write(body)
    writer.write(b"\r\n")


async def _read_json_message(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    timeout_secs: float,
) -> dict[str, object]:
    message = await _read_json_nats_message(reader, writer, timeout_secs)
    return message.payload


async def _read_json_nats_message(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    timeout_secs: float,
) -> NatsJsonMessage:
    message = await _read_message(reader, writer, timeout_secs)
    parsed = json.loads(message.payload)
    if not isinstance(parsed, dict):
        error = "NATS response JSON must be an object"
        raise NatsRpcError(error)
    return NatsJsonMessage(
        subject=message.subject,
        headers=message.headers,
        payload=cast("dict[str, object]", parsed),
    )


@dataclass(frozen=True)
class _RawMessage:
    subject: str
    headers: dict[str, str]
    payload: str


async def _read_message(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    timeout_secs: float,
) -> _RawMessage:
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout_secs)
        if line in {b"+OK\r\n", b"\r\n"}:
            continue
        if line == b"PING\r\n":
            writer.write(b"PONG\r\n")
            await writer.drain()
            continue
        if line.startswith(b"-ERR"):
            message = line.decode(errors="replace").strip()
            raise NatsRpcError(message)
        if line.startswith(b"MSG "):
            return await _read_msg_payload(reader, line, timeout_secs)
        if line.startswith(b"HMSG "):
            return await _read_hmsg_payload(reader, line, timeout_secs)
        message = f"unexpected NATS protocol line: {line!r}"
        raise NatsRpcError(message)


async def _read_msg_payload(
    reader: asyncio.StreamReader,
    line: bytes,
    timeout_secs: float,
) -> _RawMessage:
    parts = line.decode().split()
    if len(parts) < MIN_MSG_PARTS:
        message = f"malformed MSG line: {line!r}"
        raise NatsRpcError(message)
    subject = parts[1]
    size = int(parts[-1])
    payload = await asyncio.wait_for(reader.readexactly(size), timeout_secs)
    await asyncio.wait_for(reader.readexactly(2), timeout_secs)
    return _RawMessage(subject=subject, headers={}, payload=payload.decode())


async def _read_hmsg_payload(
    reader: asyncio.StreamReader,
    line: bytes,
    timeout_secs: float,
) -> _RawMessage:
    parts = line.decode().split()
    if len(parts) < MIN_HMSG_PARTS:
        message = f"malformed HMSG line: {line!r}"
        raise NatsRpcError(message)
    subject = parts[1]
    header_len = int(parts[-2])
    total_len = int(parts[-1])
    raw = await asyncio.wait_for(reader.readexactly(total_len), timeout_secs)
    await asyncio.wait_for(reader.readexactly(2), timeout_secs)
    header_bytes = raw[:header_len]
    body = raw[header_len:]
    return _RawMessage(
        subject=subject,
        headers=_parse_headers(header_bytes),
        payload=body.decode(),
    )


def _parse_headers(raw: bytes) -> dict[str, str]:
    text = raw.decode(errors="replace")
    headers: dict[str, str] = {}
    for line in text.split("\r\n")[1:]:
        if not line:
            continue
        name, sep, value = line.partition(":")
        if sep:
            headers[name.strip()] = value.strip()
    return headers
