"""Bounded pilot accounting. Software display endpoint only; no acceptance verdict."""
import json,pathlib,statistics,sys

def percentile(values):
 if not values:return {'count':0,'p50':None,'p95':None}
 v=sorted(values)
 return {'count':len(v),'p50':statistics.median(v),'p95':v[min(len(v)-1,int(.95*len(v)))]}

def summarize(usage,emitted,native,capture,trim_seconds=2):
 clock=next((x for x in capture if 'mach_numer' in x),None)
 if clock is None:raise ValueError('recorded mach timebase required')
 ns=clock['mach_numer']/clock['mach_denom'];lo=usage[0]['mach_ticks']+trim_seconds*1e9/ns;hi=usage[-1]['mach_ticks']-trim_seconds*1e9/ns
 samples=[x for x in usage if lo<=x['mach_ticks']<=hi]
 if len(samples)<2:raise ValueError('insufficient steady samples')
 first,last=samples[0],samples[-1];duration=(hi-lo)*ns/1e9
 emissions={x['sequence']:x for x in emitted if x.get('event')=='emitted'}
 receipts={x['sequence']:x['mach_ticks'] for x in emitted if x.get('event')=='input_receipt'}
 dispatch={x['input_dispatch']:x['mach_ticks'] for x in native if 'input_dispatch' in x}
 witnesses={}
 for x in native:
  if 'grid_generation' in x and x['sequences']:witnesses.setdefault(x['sequences'][0],[]).append(x)
 changed=[];duplicates=invalid=unattributed=skips=0;previous=None;seen_ack=set();receipt_latency=[];dispatch_latency=[]
 for x in capture:
  t=x.get('display_mach_ticks',-1)
  if not lo<=t<=hi or x.get('status')!=0:continue
  seqs=x.get('sequences',[])
  if not seqs:invalid+=1;continue
  s=seqs[0];e=emissions.get(s);w=witnesses.get(s,[])
  if e is None or e['mach_ticks']>t or not any(y['paint_mach_ticks']<=t and y['sequences']==seqs for y in w):unattributed+=1;continue
  if s==previous:duplicates+=1
  else:
   if previous is not None:skips+=max(0,s-previous-1)
   changed.append(t);previous=s
  if len(seqs)==2:
   ack=seqs[1]&0x7fffffff
   if ack and ack==e['ack'] and ack not in seen_ack and ack in receipts and receipts[ack]<=t:
    seen_ack.add(ack);receipt_latency.append((t-receipts[ack])*ns/1e6)
    if ack in dispatch and dispatch[ack]<=t:dispatch_latency.append((t-dispatch[ack])*ns/1e6)
 all_ack=set();all_output=set()
 for x in capture:
  seqs=x.get('sequences',[]);t=x.get('display_mach_ticks',-1)
  if x.get('status')!=0 or not seqs:continue
  seq=seqs[0];e=emissions.get(seq)
  if not e or e['mach_ticks']>t or not any(w['paint_mach_ticks']<=t and w['sequences']==seqs for w in witnesses.get(seq,[])):continue
  all_output.add(seq)
  if len(seqs)==2:
   ack=seqs[1]&0x7fffffff
   if e['ack']==ack and ack in receipts and receipts[ack]<=t:all_ack.add(ack)
 cpu=sum(last[k]-first[k] for k in ['user_mach_ticks','sys_mach_ticks'])/(last['mach_ticks']-first['mach_ticks'])*100
 return {'endpoint':'WindowServer displayTime; physical calibration UNVERIFIED','duration_seconds':duration,'changed_markers':len(changed),'changed_markers_per_second':len(changed)/duration,'duplicate_markers':duplicates,'invalid_complete':invalid,'unattributed':unattributed,'skipped_emitted_sequences':skips,'interval_ms':percentile([(b-a)*ns/1e6 for a,b in zip(changed,changed[1:])]),'ack_receipt_to_display_ms':percentile(receipt_latency),'ack_dispatch_to_display_ms':percentile(dispatch_latency),'emitted_sequences_total':len(emissions),'matched_output_sequences_full_capture':len(all_output),'unobserved_emissions_including_coalesced_and_outside_capture':len(set(emissions)-all_output),'final_emission_observed':max(emissions) in all_output if emissions else None,'ack_matched_full_capture':len(all_ack),'ack_unmatched_full_capture':len(set(receipts)-all_ack),'ack_dispatches_without_receipt':len(set(dispatch)-set(receipts)),'ack_dispatches_total':len(dispatch),'ack_dispatches_without_receipt_ids':sorted(set(dispatch)-set(receipts)),'ack_dispatches_without_presentation_ids':sorted(set(dispatch)-all_ack),'unmatched_receipts_total_including_outside_trim':len(set(receipts)-seen_ack),'unmatched_receipts_in_trim':len({k for k,v in receipts.items() if lo<=v<=hi}-seen_ack),'receipts_outside_trim':sum(not lo<=v<=hi for v in receipts.values()),'cpu_percent':cpu,'user_cpu_percent':(last['user_mach_ticks']-first['user_mach_ticks'])/(last['mach_ticks']-first['mach_ticks'])*100,'system_cpu_percent':(last['sys_mach_ticks']-first['sys_mach_ticks'])/(last['mach_ticks']-first['mach_ticks'])*100,'cpu_interval_seconds':(last['mach_ticks']-first['mach_ticks'])*ns/1e9,'footprint_median_bytes':statistics.median(x['footprint'] for x in samples),'footprint_max_bytes':max(x['footprint'] for x in samples),'kernel_energy_joules':(last['energy_nj']-first['energy_nj'])/1e9,'energy_coverage':'UNVERIFIED; kernel per-task accounting, not battery energy','continuous_visibility':'UNVERIFIED','capture_loss':'UNVERIFIED; skipped producer sequences are not capture loss'}

def records(path):
 if not path.exists():return []
 result=[]
 for line in path.read_text().splitlines():
  try:result.append(json.loads(line))
  except ValueError:pass
 return result
if __name__=='__main__':
 p=pathlib.Path(sys.argv[1]);native=records(p/'native.jsonl');meta=json.loads((p/'manifest.json').read_text())
 capture=records(p/'capture.jsonl')
 result=summarize(records(p/'usage.jsonl'),records(p/'emissions.jsonl'),native,capture+[meta])
 if not capture:result['endpoint']='not captured; resource accounting only'
 result['frame_misses']='UNVERIFIED: eligible display opportunities unavailable'
 environments=[x for x in native if 'environment_mach_ticks' in x]
 observed=[x for x in native if 'root_probe_seconds' in x and x['root_probe_seconds']>=2]
 reasons=[]
 if any(not x['visible'] or not x['active'] for x in observed) or any(x.get('observed_visibility_seconds',0)>=1 and not x['visible'] for x in native if 'observed_visibility_seconds' in x):reasons.append('observed hidden/inactive; reject active comparison')
 if any(str(x['display'])!=meta['parameters']['display'] for x in environments):reasons.append('wrong display')
 if any(x['power'] is not False or x['thermal'] not in ['Nominal','Fair'] for x in environments):reasons.append('power/thermal ineligible')
 if any(meta.get(k,0)!=0 for k in ['native_exit','usage_exit','capture_exit']):reasons.append('process failure')
 if any(x.get('record_overflow',0)>0 for x in capture) or any(x.get('grid_record_overflow',0)>0 for x in native):reasons.append('record overflow')
 if meta['parameters']['mode'].startswith('stripe') and (result['invalid_complete'] or result['unattributed']):reasons.append('invalid or unattributed steady frames')
 result['decoding_enabled']=meta['parameters']['mode'].startswith('stripe')
 if not result['decoding_enabled']:result['not_decoded_complete']=result['invalid_complete'];result['invalid_complete']=None
 result.update(rejection_reasons=reasons,native_observed_samples=len(observed),gpu_ns_delta=(environments[-1]['gpu_ns']-environments[0]['gpu_ns'] if len(environments)>1 and all(x['gpu_ns'] is not None for x in environments) else None),gpu_status='UNVERIFIED unless nonzero and calibrated',capture_summary=next((x for x in records(p/'capture.jsonl') if x.get('summary')),None))
 capture_usage=[x for x in records(p/'capture-usage.jsonl') if 'mach_ticks' in x]
 if len(capture_usage)>20:
  own=summarize(capture_usage,[],[],[meta])
  result['capture_process']={k:own[k] for k in ['cpu_percent','user_cpu_percent','system_cpu_percent','footprint_median_bytes']}
 print(json.dumps(result,indent=2))
