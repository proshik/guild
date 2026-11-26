// Read-only accounting of one owned subject; verify birth at every sample.
#include <libproc.h>
#include <sys/resource.h>
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(int argc,char **argv) {
    if(argc!=2)return 64;
    pid_t pid=(pid_t)atoi(argv[1]);if(pid<=0)return 64;
    mach_timebase_info_data_t tb; if(mach_timebase_info(&tb)!=0 || !tb.denom)return 1;
    uint64_t birth=0;
    for(int i=0;i<420;i++) {
        struct rusage_info_v4 r={0};struct proc_taskinfo task={0};
        if(proc_pid_rusage(pid,RUSAGE_INFO_V4,(rusage_info_t *)&r)!=0)return 0;
        if(!birth)birth=r.ri_proc_start_abstime;else if(birth!=r.ri_proc_start_abstime)return 2;
        int bytes=proc_pidinfo(pid,PROC_PIDTASKINFO,0,&task,sizeof(task));
        printf("{\"wall_ticks\":%llu,\"birth_ticks\":%llu,\"numer\":%u,\"denom\":%u,\"user_ticks\":%llu,\"system_ticks\":%llu,\"reaped_child_user_ticks\":%llu,\"reaped_child_system_ticks\":%llu,\"footprint\":%llu,\"threads\":%d}\n",mach_absolute_time(),birth,tb.numer,tb.denom,r.ri_user_time,r.ri_system_time,r.ri_child_user_time,r.ri_child_system_time,r.ri_phys_footprint,bytes==sizeof(task)?task.pti_threadnum:-1);
        fflush(stdout);sleep(1);
    }
    return 3;
}
