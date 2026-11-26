import Foundation
import ScreenCaptureKit
import CoreGraphics
import CoreMedia
import CoreVideo
final class Sink: NSObject, SCStreamOutput, @unchecked Sendable {
    var decodeEnabled=true
    var expectedStripes=1
    var sampled=false
    var geometry:[Stripe]=[], records:[[String:Any]]=[], droppedRecords=0
    var accounting=FrameAccounting(), statuses:[Int:Int]=[:], lastScan:UInt64=0
    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen else {return}
        let attachments=CMSampleBufferGetSampleAttachmentsArray(sample,createIfNecessary:false) as? [[SCStreamFrameInfo:Any]]
        let info=attachments?.first ?? [:]
        let status=(info[.status] as? NSNumber)?.intValue ?? -1
        let display=(info[.displayTime] as? NSNumber)?.uint64Value ?? 0
        var sequences:[UInt32]=[]
        if decodeEnabled, status==0, let image=CMSampleBufferGetImageBuffer(sample) {
            CVPixelBufferLockBaseAddress(image,.readOnly)
            let width=CVPixelBufferGetWidth(image),height=CVPixelBufferGetHeight(image),stride=CVPixelBufferGetBytesPerRow(image)
            if let bytes=CVPixelBufferGetBaseAddress(image)?.assumingMemoryBound(to:UInt8.self) {
                let pixel:(Int,Int)->FrameCodec.Pixel = {x,y in let p=y*stride+x*4;return (bytes[p+2],bytes[p+1],bytes[p])}
                if !sampled {
                    sampled=true
                    var counts:[String:Int]=[:];var colored:[[Int]]=[]
                    for y in Swift.stride(from:0,to:height,by:4) {for x in Swift.stride(from:0,to:width,by:4) {
                        let c=pixel(x,y);let key="\(c.0),\(c.1),\(c.2)";counts[key,default:0]+=1
                        if FrameCodec.kind(c)==2 && colored.count<12 {colored.append([x,y])}
                    }}
                    if let xy=colored.first {
                        let y=xy[1];var left=xy[0];while left>0 && FrameCodec.kind(pixel(left-1,y))==2 {left-=1}
                        var end=left;while end<width && FrameCodec.kind(pixel(end,y))==2 {end+=1}
                        var tail=end;while tail<width && FrameCodec.kind(pixel(tail,y)) != 3 {tail+=1}
                        var tailEnd=tail;while tailEnd<width && FrameCodec.kind(pixel(tailEnd,y))==3 {tailEnd+=1}
                        let cell=Double(tailEnd-left)/52
                        let samples=(0..<52).map {i->[Int] in let x=min(width-1,Int(Double(left)+(Double(i)+0.5)*cell));let c=pixel(x,y);return [x,Int(c.0),Int(c.1),Int(c.2),FrameCodec.kind(c)]}
                        records.append(["candidate":[left,y,end,tail,tailEnd],"cell":cell,"samples":samples])
                    }
                    records.append(["pixel_format":CVPixelBufferGetPixelFormatType(image),"top_colors":counts.sorted{$0.value>$1.value}.prefix(12).map{["rgb":$0.key,"count":$0.value] as [String:Any]},"magenta_samples":colored])
                }
                sequences=geometry.compactMap {FrameCodec.decode($0,width:width,height:height,pixel:pixel)}
                // No sample-count cutoff: recovery scans continue throughout activity.
                if sequences.count < expectedStripes && accounting.received-lastScanInt()>=30 {
                    geometry=FrameCodec.locate(width:width,height:height,pixel:pixel)
                    records.append(["geometry":geometry.map{["x":$0.x,"y":Double($0.y),"cell":$0.cell]}])
                    lastScan=UInt64(accounting.received)
                    sequences=geometry.compactMap {FrameCodec.decode($0,width:width,height:height,pixel:pixel)}
                }
            }
            CVPixelBufferUnlockBaseAddress(image,.readOnly)
        }
        accounting.add(status:status,sequence:sequences.first(where:{$0 & 0x80000000 == 0}))
        statuses[status,default:0]+=1
        if records.count<250000 {records.append(["status":status,"display_mach_ticks":display,"callback_mach_ticks":mach_absolute_time(),"sequences":sequences])} else {droppedRecords+=1}
    }
    func lastScanInt()->Int {Int(lastScan)}
}
@main struct Probe {
    static func main() async throws {
        guard (3...4).contains(CommandLine.arguments.count),let pid=Int32(CommandLine.arguments[1]),let seconds=UInt64(CommandLine.arguments[2]),seconds<=600 else {exit(64)}
        _=CGMainDisplayID()
        guard CGPreflightScreenCaptureAccess() else {print("{\"permission\":false}");exit(78)}
        var owned:[SCWindow]=[]
        // Newly activated windows can precede ScreenCaptureKit's content snapshot.
        // Retry absence only, never ambiguity or permission denial.
        for attempt in 0..<4 {
            let content=try await SCShareableContent.excludingDesktopWindows(true,onScreenWindowsOnly:true)
            owned=content.windows.filter {$0.owningApplication?.processID==pid}
            if !owned.isEmpty {break}
            if attempt<3 {try await Task.sleep(nanoseconds:250_000_000)}
        }
        guard owned.count==1, let window=owned.first else {print("owned window absent or ambiguous: \(owned.count)");exit(69)}
        let config=SCStreamConfiguration();config.minimumFrameInterval = .zero
        config.width=Int(window.frame.width)*2;config.height=Int(window.frame.height)*2
        config.pixelFormat=kCVPixelFormatType_32BGRA;config.queueDepth=8;config.showsCursor=false
        let sink=Sink();sink.decodeEnabled=CommandLine.arguments.count==3 || CommandLine.arguments[3] != "counts"
        sink.expectedStripes=CommandLine.arguments.last == "ack" ? 2 : 1
        let queue=DispatchQueue(label:"owned.stripe.capture")
        let stream=SCStream(filter:SCContentFilter(desktopIndependentWindow:window),configuration:config,delegate:nil)
        try stream.addStreamOutput(sink,type:.screen,sampleHandlerQueue:queue)
        var timebase=mach_timebase_info_data_t();mach_timebase_info(&timebase)
        emit(["endpoint":"WindowServer SCStreamFrameInfo.displayTime (software)","pid":pid,"window_id":window.windowID,"mach_numer":timebase.numer,"mach_denom":timebase.denom,"capture_width":config.width,"capture_height":config.height,"physical_calibration":"UNVERIFIED","decoding_enabled":sink.decodeEnabled])
        try await stream.startCapture();try await Task.sleep(nanoseconds:seconds*1_000_000_000);try await stream.stopCapture()
        queue.sync {
            for record in sink.records {emit(record)}
            let a=sink.accounting
            emit(["summary":true,"received":a.received,"complete":a.complete,"idle":a.idle,"invalid":a.invalid,"duplicate":a.duplicate,"changed":a.changed,"skipped_sequences":a.skippedSequences,"record_overflow":sink.droppedRecords,"statuses":Dictionary(uniqueKeysWithValues:sink.statuses.map {(String($0.key),$0.value)}),"capture_internal_loss":"UNVERIFIED: public endpoint has no sequence/drop counter"])
        }
    }
    static func emit(_ record:[String:Any]) {if let data=try? JSONSerialization.data(withJSONObject:record,options:.sortedKeys),let s=String(data:data,encoding:.utf8){print(s)}}
}
