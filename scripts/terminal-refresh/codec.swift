import Foundation
struct Stripe { let x: Double; let y: Int; let cell: Double }
struct FrameCodec {
    typealias Pixel = (UInt8, UInt8, UInt8)
    static func kind(_ p: Pixel) -> Int {
        let (r,g,b)=p
        if r>180 && b>180 && Int(min(r,b))>Int(g)+80 {return 2}
        if r>180 && g>180 && Int(min(r,g))>Int(b)+80 {return 3}
        if g>180 && Int(g)>Int(r)+60 && Int(g)>Int(b)+60 {return 1}
        if b>160 && Int(b)>Int(r)+60 && Int(b)>Int(g)+60 {return 0}
        return -1
    }
    static func crc(_ seq: UInt32) -> UInt16 {
        var c: UInt16=0xffff
        for shift in [24,16,8,0] {
            c ^= UInt16((seq >> shift) & 255) << 8
            for _ in 0..<8 {c = c & 0x8000 != 0 ? (c &<< 1) ^ 0x1021 : c &<< 1}
        }
        return c
    }
    static func decode(_ at: Stripe, width: Int, height: Int, pixel: (Int,Int)->Pixel) -> UInt32? {
        guard at.cell>=2, at.x>=0, at.y>=0, at.y<height, at.x+52*at.cell<=Double(width) else {return nil}
        func sample(_ i: Int)->Int {kind(pixel(Int(at.x+(Double(i)+0.5)*at.cell),at.y))}
        guard sample(0)==2 && sample(1)==2 && sample(50)==3 && sample(51)==3 else {return nil}
        var value: UInt64=0
        for i in 2..<50 {let b=sample(i);guard b==0 || b==1 else {return nil};value=(value<<1)|UInt64(b)}
        let seq=UInt32(value>>16)
        return UInt16(value & 65535)==crc(seq) ? seq : nil
    }
    static func locate(width: Int,height: Int,pixel: (Int,Int)->Pixel)->[Stripe] {
        var found:[Stripe]=[];var roles:Set<UInt32>=[]
        // A bounded initial/recovery scan. Normal frames sample only 104 centers.
        for y in stride(from:2,to:height,by:4) {
            var x=0
            while x<width-104 {
                if kind(pixel(x,y)) != 2 {x+=2;continue}
                var left=x;while left>0 && kind(pixel(left-1,y))==2 {left-=1}
                var end=x;while end<width && kind(pixel(end,y))==2 {end+=1}
                var tail=end
                while tail<width && kind(pixel(tail,y)) != 3 {tail+=1}
                var tailEnd=tail
                while tailEnd<width && kind(pixel(tailEnd,y))==3 {tailEnd+=1}
                let cell=Double(tailEnd-left)/52
                let at=Stripe(x:Double(left),y:y,cell:cell)
                if abs(Double(end-left)-2*cell)<=3, let value=decode(at,width:width,height:height,pixel:pixel) {
                    let role=value & 0x80000000
                    if !roles.contains(role) {found.append(at);roles.insert(role)}
                    break
                }
                x=end+2
            }
            if found.count==2 {break}
        }
        return found
    }
}
struct FrameAccounting {
    var received=0, complete=0, idle=0, invalid=0, duplicate=0, changed=0, skippedSequences: UInt64=0
    var last: UInt32?
    mutating func add(status: Int, sequence: UInt32?) {
        received+=1
        if status==1 {idle+=1;return}
        guard status==0 else {return}
        complete+=1
        guard let s=sequence else {invalid+=1;return}
        if let p=last {if p==s {duplicate+=1;return};if s>p {skippedSequences+=UInt64(s-p-1)}}
        changed+=1;last=s
    }
}
