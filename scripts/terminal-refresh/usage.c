// Public per-owned-process accounting; unsupported counters are not energy proof.
#include <libproc.h>
#include <sys/resource.h>
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
int main(int argc,char **argv) {
    if(argc!=3)return 64;
    pid_t pid=(pid_t)atoi(argv[1]);int samples=atoi(argv[2]);
    if(pid<=0 || samples<1 || samples>2400)return 64;
    uint64_t identity=0;
    for(int i=0;i<samples;i++) {
        struct rusage_info_v6 r={0};
        int rc=proc_pid_rusage(pid,RUSAGE_INFO_V6,(rusage_info_t *)&r);
        if(rc!=0){printf("{\"rusage_error\":%d,\"errno\":%d}\n",rc,errno);return 1;}
        if(i==0)identity=r.ri_proc_start_abstime;
        else if(identity!=r.ri_proc_start_abstime)return 2;
        printf("{\"mach_ticks\":%llu,\"user_mach_ticks\":%llu,\"sys_mach_ticks\":%llu,\"footprint\":%llu,\"energy_nj\":%llu,\"billed_energy_raw\":%llu,\"start_ticks\":%llu}\n",mach_absolute_time(),r.ri_user_time,r.ri_system_time,r.ri_phys_footprint,r.ri_energy_nj,r.ri_billed_energy,r.ri_proc_start_abstime);
        fflush(stdout);usleep(250000);
    }
    return 0;
}
