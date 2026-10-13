"""Cliente mínimo do protocolo HID do Redragon M808-KS (ver NOTAS-mouse-redragon.md).

Por enquanto só leitura: handshake (0x01) e ReadEEPROM (0x08).
"""
import random
import sys
import time

import hid

VID = 0x25A7
PIDS = (0xFA08, 0xFA07)
PSD = (5, 0, 0, 0)  # chave do handshake (Cfg.ini PSD)


def checksum(buf16):
    return (0x55 - sum(buf16)) & 0xFF


class M808:
    def __init__(self):
        self.cmd = self.notify = None
        for pid in PIDS:
            devs = hid.enumerate(VID, pid)
            c = next((d for d in devs if d["usage_page"] == 0xFF02 and d["usage"] == 0x02), None)
            n = next((d for d in devs if d["usage_page"] == 0xFF01 and d["usage"] == 0x00), None)
            if c and n:
                self.pid = pid
                self.cmd = hid.device()
                self.cmd.open_path(c["path"])
                self.notify = hid.device()
                self.notify.open_path(n["path"])
                break
        if not self.cmd:
            raise RuntimeError("mouse não encontrado")

    def close(self):
        self.cmd.close()
        self.notify.close()

    def _drain(self):
        self.notify.set_nonblocking(1)
        while self.notify.read(64):
            pass
        self.notify.set_nonblocking(0)

    def send(self, cmd, payload=b"", timeout_ms=500, retries=4):
        """Envia [08 cmd payload.. chk] e espera a resposta com eco do cmd.

        O mouse sem fio pode estar dormindo; o dongle responde a 1..4 mas comandos que vão
        até o mouse (ex.: 0x08) somem nesse caso, por isso o reenvio.
        """
        for _ in range(retries):
            r = self._send_once(cmd, payload, timeout_ms)
            if r is not None:
                return r
        return None

    def _send_once(self, cmd, payload, timeout_ms):
        pkt = bytearray(16)
        pkt[0] = 0x08
        pkt[1] = cmd
        pkt[2:2 + len(payload)] = payload
        self._drain()
        self.cmd.send_feature_report(bytes(pkt) + bytes([checksum(pkt)]))
        deadline = time.time() + timeout_ms / 1000
        while time.time() < deadline:
            r = self.notify.read(64, int((deadline - time.time()) * 1000) or 1)
            if not r:
                continue
            if r[1] == 0x0A:  # evento espontâneo do mouse
                continue
            if r[1] == cmd:
                return bytes(r)
        return None

    def handshake(self):
        rnd = [random.randint(1, 100) for _ in range(4)]
        payload = bytes([0, 0, 0, 0x04] + rnd)
        r = self.send(0x01, payload)
        k = PSD
        exp = [(k[0] + rnd[0] + rnd[1]) & 0xFF, (k[1] + 2 * rnd[1] + rnd[2]) & 0xFF,
               (k[2] + 3 * rnd[2] + rnd[3]) & 0xFF, (k[3] + 4 * rnd[3] + rnd[0]) & 0xFF]
        return r, exp

    def read_eeprom(self, addr, length):
        out = bytearray()
        while length > 0:
            n = min(10, length)
            r = self.send(0x08, bytes([0, addr >> 8, addr & 0xFF, n]))
            if r is None or r[2] != 0:
                raise IOError(f"ReadEEPROM falhou em 0x{addr:04x}: {r.hex() if r else 'timeout'}")
            out += r[6:6 + n]
            addr += n
            length -= n
        return bytes(out)


    def write_eeprom(self, addr, data):
        data = bytes(data)
        while data:
            chunk = data[:10]
            r = self.send(0x07, bytes([0, addr >> 8, addr & 0xFF, len(chunk)]) + chunk)
            if r is None or r[2] != 0:
                raise IOError(f"WriteEEPROM falhou em 0x{addr:04x}: {r.hex() if r else 'timeout'}")
            addr += len(chunk)
            data = data[10:]


def entry(*vals):
    """Entrada com checksum final: [v..., 0x55 - soma]."""
    return bytes(vals) + bytes([(0x55 - sum(vals)) & 0xFF])


def hexdump(base, data):
    for i in range(0, len(data), 16):
        chunk = data[i:i + 16]
        print(f"{base + i:04x}: {chunk.hex(' ')}")


if __name__ == "__main__":
    m = M808()
    print(f"aberto PID 0x{m.pid:04x}")
    try:
        if "--handshake" in sys.argv:
            r, exp = m.handshake()
            print("handshake resp:", r.hex(" ") if r else None, "esperado[6..9]:", bytes(exp).hex(" "))
        start = int(sys.argv[1], 0) if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else 0
        size = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0x100
        hexdump(start, m.read_eeprom(start, size))
    finally:
        m.close()
