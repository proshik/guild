import unittest
from analysis import summarize
class Accounting(unittest.TestCase):
 def test_matched_frames_and_ack(self):
  usage=[{'mach_ticks':t,'user_mach_ticks':t//10,'sys_mach_ticks':0,'footprint':100,'energy_nj':t,'start_ticks':1} for t in [0,100,200,300,400,500,600]]
  emitted=[{'event':'emitted','sequence':s,'ack':a,'mach_ticks':t} for s,a,t in [(1,0,210),(3,1,310)]]
  emitted.append({'event':'input_receipt','sequence':1,'mach_ticks':300})
  native=[{'grid_generation':g,'paint_mach_ticks':t,'sequences':seq} for g,t,seq in [(4,215,[1]),(8,315,[3,0x80000001])]]
  native.append({'input_dispatch':1,'mach_ticks':290})
  capture=[{'mach_numer':10000000,'mach_denom':1},*({'status':0,'display_mach_ticks':t,'sequences':s} for t,s in [(220,[1]),(230,[1]),(320,[3,0x80000001]),(330,[99])])]
  r=summarize(usage,emitted,native,capture,trim_seconds=2)
  self.assertEqual(r['changed_markers'],2);self.assertEqual(r['duplicate_markers'],1);self.assertEqual(r['unattributed'],1)
  self.assertEqual(r['skipped_emitted_sequences'],1);self.assertAlmostEqual(r['cpu_percent'],10)
  self.assertEqual(r['ack_receipt_to_display_ms']['p50'],200)
  self.assertEqual(r['ack_dispatch_to_display_ms']['p50'],300)
 def test_missing_dispatches_remain_missing_when_all_receipts_presented(self):
  u=[{'mach_ticks':t,'user_mach_ticks':0,'sys_mach_ticks':0,'footprint':1,'energy_nj':0,'start_ticks':1} for t in [0,100]]
  e=[{'event':'emitted','sequence':1,'ack':1,'mach_ticks':20},{'event':'input_receipt','sequence':1,'mach_ticks':10}]
  n=[{'input_dispatch':i,'mach_ticks':5} for i in [1,2,3]]+[{'grid_generation':1,'paint_mach_ticks':25,'sequences':[1,0x80000001]}]
  c=[{'mach_numer':1,'mach_denom':1},{'status':0,'display_mach_ticks':30,'sequences':[1,0x80000001]}]
  r=summarize(u,e,n,c,trim_seconds=0)
  self.assertEqual(r['ack_unmatched_full_capture'],0)
  self.assertEqual(r['ack_dispatches_without_receipt_ids'],[2,3])
  self.assertEqual(r['ack_dispatches_without_presentation_ids'],[2,3])
  self.assertEqual(r['ack_dispatches_total'],3)
 def test_wrong_generation_is_not_display_evidence(self):
  u=[{'mach_ticks':t,'user_mach_ticks':0,'sys_mach_ticks':0,'footprint':1,'energy_nj':0,'start_ticks':1} for t in [0,100]]
  r=summarize(u,[{'event':'emitted','sequence':1,'ack':0,'mach_ticks':10}],[{'grid_generation':2,'paint_mach_ticks':90,'sequences':[1]}],[{'mach_numer':10000000,'mach_denom':1},{'status':0,'display_mach_ticks':20,'sequences':[1]}],trim_seconds=0)
  self.assertEqual(r['changed_markers'],0);self.assertEqual(r['unattributed'],1)
if __name__=='__main__':unittest.main()
