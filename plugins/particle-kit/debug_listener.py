import win32pipe, win32file
import sys

pipe_name = r'\.\pipe\AEExternalDebug'
print(f"Creating named pipe: {pipe_name}")
pipe = win32pipe.CreateNamedPipe(
    pipe_name,
    win32pipe.PIPE_ACCESS_INBOUND,
    win32pipe.PIPE_TYPE_BYTE | win32pipe.PIPE_READMODE_BYTE | win32pipe.PIPE_WAIT,
    1, 65536, 65536, 0, None
)

print("Waiting for connection... (switch to AE and scrub timeline)")
try:
    while True:
        win32pipe.ConnectNamedPipe(pipe, None)
        data = b""
        while True:
            try:
                result, chunk = win32file.ReadFile(pipe, 4096)
                data += chunk
            except:
                break
        if data:
            for line in data.decode('utf-8', errors='replace').strip().split('\n'):
                print(line)
        win32pipe.DisconnectNamedPipe(pipe)
except KeyboardInterrupt:
    pass
finally:
    win32file.CloseHandle(pipe)
