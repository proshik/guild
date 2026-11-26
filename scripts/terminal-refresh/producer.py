"""Owned synthetic PTY only. Sequence/CRC stripes, buffered mach-clock records."""
import binascii, ctypes, json, os, select, struct, sys, time, tty

def stripe(sequence, row):
    raw=struct.pack('>I',sequence)
    bits=''.join(f'{b:08b}' for b in raw+struct.pack('>H',binascii.crc_hqx(raw,0xffff)))
    cells='\x1b[48;2;240;16;240m  '+''.join(('\x1b[48;2;16;240;16m' if b=='1' else '\x1b[48;2;16;16;240m')+' ' for b in bits)+'\x1b[48;2;240;240;16m  \x1b[0m'
    return f'\x1b[{row};1H'+cells

def main():
    clock=ctypes.CDLL('/usr/lib/libSystem.B.dylib').mach_absolute_time
    clock.restype=ctypes.c_uint64
    tty.setraw(0)
    mode=os.environ.get('GUILD_PROBE_MODE','stripe')
    seconds=float(os.environ.get('GUILD_PROBE_SECONDS','12'))
    records=[];line=b'';sequence=0;ack=0
    sys.stdout.write('\x1b[?25l\x1b[2J');sys.stdout.flush()
    start=time.monotonic();deadline=start;end=start+seconds-1
    while time.monotonic()<end:
        ready,_,_=select.select([0],[],[],max(0,deadline-time.monotonic()))
        if ready:
            line+=os.read(0,1024).replace(b'\r',b'\n')
            while b'\n' in line:
                value,line=line.split(b'\n',1)
                if value.isdigit():
                    ack=int(value);records.append({'event':'input_receipt','sequence':ack,'mach_ticks':clock()})
        now=time.monotonic()
        if now>=deadline:
            sequence+=1
            records.append({'event':'emitted','sequence':sequence,'ack':ack,'mach_ticks':clock()})
            sys.stdout.write('\x1b[?2026h'+stripe(sequence,1)+(stripe(ack|0x80000000,3) if mode=='stripe-ack' else '')+'\x1b[?2026l');sys.stdout.flush()
            deadline=max(deadline+1/240,time.monotonic())
    path=os.environ.get('GUILD_PROBE_EMISSIONS')
    if path:
        with open(path,'w') as f:
            for r in records:f.write(json.dumps(r)+'\n')
    time.sleep(2)
if __name__=='__main__':main()
