#include <dispatch/dispatch.h>
#include <mach/mach_time.h>
#include <stdio.h>
struct C { dispatch_semaphore_t done; dispatch_source_t source; uint64_t fired; };
static void after(void *p){struct C*c=p;c->fired=mach_absolute_time();dispatch_semaphore_signal(c->done);}
static void fire(void *p){struct C*c=p;c->fired=mach_absolute_time();dispatch_source_cancel(c->source);}
static void cancel(void *p){struct C*c=p;dispatch_semaphore_signal(c->done);}
int main(void){mach_timebase_info_data_t tb;mach_timebase_info(&tb);for(int mode=0;mode<3;mode++)for(int i=0;i<150;i++){
 struct C c={.done=dispatch_semaphore_create(0)};uint64_t start=mach_absolute_time();dispatch_queue_t q=dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH,0);
 if(mode==0)dispatch_after_f(dispatch_time(DISPATCH_TIME_NOW,4000000),q,&c,after);
 else{c.source=dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER,0,mode==2?DISPATCH_TIMER_STRICT:0,q);dispatch_set_context(c.source,&c);dispatch_source_set_event_handler_f(c.source,fire);dispatch_source_set_cancel_handler_f(c.source,cancel);dispatch_source_set_timer(c.source,dispatch_time(DISPATCH_TIME_NOW,4000000),DISPATCH_TIME_FOREVER,0);dispatch_resume(c.source);}
 dispatch_semaphore_wait(c.done,DISPATCH_TIME_FOREVER);printf("%d,%llu\n",mode,(c.fired-start)*tb.numer/tb.denom);if(c.source)dispatch_release(c.source);dispatch_release(c.done);
 }return 0;}
