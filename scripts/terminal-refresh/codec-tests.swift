import Foundation
@main struct Tests {
    static func main() {
        // Independent fixture bytes: CRC-CCITT-FALSE(12 34 56 78) = 30EC.
        let bytes: [UInt8] = [0x12,0x34,0x56,0x78,0x30,0xEC]
        var bits = bytes.flatMap { b in (0..<8).reversed().map { (b >> $0) & 1 } }
        let magenta: FrameCodec.Pixel=(240,16,240), yellow: FrameCodec.Pixel=(240,240,16)
        var corrupt = false
        let pixel: (Int,Int)->FrameCodec.Pixel = { x,_ in
            let cell=x/8
            if cell<2 {return magenta};if cell>=50 {return yellow}
            let bit=bits[cell-2] ^ (corrupt && cell==9 ? 1 : 0)
            return bit==1 ? (16,240,16) : (16,16,240)
        }
        let stripe=Stripe(x:0,y:4,cell:8)
        precondition(FrameCodec.decode(stripe,width:416,height:8,pixel:pixel)==0x12345678,"valid sequence/CRC rejected")
        let managed:(Int,Int)->FrameCodec.Pixel = {x,y in
            let p=pixel(x,y);if p.0==240 && p.1==240 {return (240,240,81)};if p.0==240 && p.2==240 {return (220,52,232)};return p.1==240 && p.0==16 ? (111,237,74) : p
        }
        precondition(FrameCodec.decode(stripe,width:416,height:8,pixel:managed)==0x12345678,"managed display color rejected")
        for cellWidth in [14.2,15.2,15.6,16.4] {
        let fractional:(Int,Int)->FrameCodec.Pixel = {x,y in
            let cell=Int(floor((Double(x)+0.5-10.3)/cellWidth))
            if y<4 || y>=20 || cell<0 || cell>=52 {return (0,0,0)}
            return pixel(cell*8+4,4)
        }
        precondition(!FrameCodec.locate(width:900,height:40,pixel:fractional).isEmpty,"fractional terminal cells were not located")
        }
        corrupt=true
        precondition(FrameCodec.decode(stripe,width:416,height:8,pixel:pixel)==nil,"torn payload accepted")
        corrupt=false
        precondition(FrameCodec.decode(stripe,width:400,height:8,pixel:pixel)==nil,"truncated sentinel accepted")
        precondition(FrameCodec.decode(stripe,width:416,height:8,pixel:{_,_ in (0,0,0)})==nil,"blank frame accepted")
        let rows:(Int,Int)->FrameCodec.Pixel = {x,y in
            if (4..<42).contains(y) {return pixel(x,4)}
            if (42..<118).contains(y) {
                let cell=x/8
                if cell<2 {return magenta};if cell>=50 {return yellow}
                let value=(UInt64(0x80000001)<<16)|UInt64(FrameCodec.crc(0x80000001))
                return (value >> (49-cell)) & 1 == 1 ? (16,240,16) : (16,16,240)
            }
            return (0,0,0)
        }
        let located=FrameCodec.locate(width:416,height:120,pixel:rows)
        precondition(located.count==2 && located[1].y>=42,"two samples of one stripe mistaken for output and ACK")
        let staleNeighbor:(Int,Int)->FrameCodec.Pixel = {x,y in
            if (4..<24).contains(y) {return pixel(x,4)}
            if (24..<64).contains(y) {return (x<16 ? magenta : (0,0,0))}
            return (0,0,0)
        }
        let top=FrameCodec.locate(width:416,height:70,pixel:staleNeighbor)
        precondition(top.first.map{FrameCodec.decode($0,width:416,height:70,pixel:staleNeighbor)} == 0x12345678,"vertical sentinel continuation changed the validated sample row")
        bits[47] ^= 1
        precondition(FrameCodec.decode(stripe,width:416,height:8,pixel:pixel)==nil,"bad checksum accepted")
        var a=FrameAccounting()
        a.add(status:0,sequence:1);a.add(status:0,sequence:1);a.add(status:0,sequence:nil);a.add(status:1,sequence:nil);a.add(status:0,sequence:4)
        precondition(a.received==5 && a.changed==2 && a.duplicate==1 && a.invalid==1 && a.idle==1 && a.skippedSequences==2)
        print("codec and frame accounting cases passed")
    }
}
