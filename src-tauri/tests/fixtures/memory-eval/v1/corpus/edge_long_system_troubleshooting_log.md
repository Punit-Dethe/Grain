---
grain_id: "m_edge_long_log_34"
title: "Distributed cluster troubleshooting incident log"
tldr: "Comprehensive diagnostic trace of network packet drops and MTU mismatch on node 105."
question: "what MTU mismatch caused packet drops on node 192.168.1.105?"
entities: [troubleshooting, network, cluster incident, MTU, packet drop]
created: 2026-07-28T18:00:00.000
source: selection
---
INCIDENT DIAGNOSTIC ARCHIVE — CLUSTER NODE FAILURE:
Timestamp: 2026-07-28T14:22:01.458Z
Severity: P1 Critical
Impact: 4% of ingress TCP payloads experiencing intermittent SYN/ACK timeouts.

SYSTEM TOPOLOGY:
- Hostname: k8s-worker-gpu-04.internal.cluster.net
- Internal IPv4: 192.168.1.105
- Primary NIC: eth0 (Mellanox ConnectX-6 Dx 100GbE)
- Virtual Switch: Open vSwitch 3.1.2 with DPDK acceleration
- Kernel: Linux 6.8.0-42-generic #42-Ubuntu SMP PREEMPT_DYNAMIC

DIAGNOSTIC LOG EXCERPTS:
[14:22:05.102] kernel: [ 4512.981023] eth0: tx_timeout occurred on queue 3, resetting hardware ring buffer
[14:22:06.331] systemd-networkd[1104]: eth0: Lost carrier
[14:22:07.892] ovs-vswitchd[2455]: netdev_dpdk|WARN|eth0: rx packet size (9000) exceeds interface MTU (1500)
[14:22:08.012] kubelet[3100]: E0728 14:22:08.012000 pod_workers.go:1290] "Error syncing pod" err="network: failed to configure bridge: MTU mismatch on veth interface"
[14:22:09.554] envoy[4501]: [warning][upstream] [source/common/upstream/cluster_manager_impl.cc:1145] host 192.168.1.105:8443 consecutive gateway connection failures reaching threshold 5, ejecting from pool
[14:22:11.201] kernel: [ 4518.234190] TCP: eth0: Packet drop rate on port 8443 exceeded 12000 packets/sec
[14:22:12.788] dpkg[15001]: installed package ethtool 1:5.16-1 successfully
[14:22:14.020] root# ip link show eth0
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DEFAULT group default qlen 1000
    link/ether 00:0a:f7:23:45:67 brd ff:ff:ff:ff:ff:ff
    altname enp3s0f0
[14:22:15.890] root# ip link set dev eth0 mtu 9000
[14:22:16.102] kernel: [ 4523.001923] eth0: Link MTU set to 9000 bytes (jumbo frames enabled)
[14:22:18.450] ovs-vsctl set interface eth0 mtu_request=9000
[14:22:20.991] ping -M do -s 8972 -c 5 192.168.1.1
PING 192.168.1.1 (192.168.1.1) 8972(9000) bytes of data.
8980 bytes from 192.168.1.1: icmp_seq=1 ttl=64 time=0.045 ms
8980 bytes from 192.168.1.1: icmp_seq=2 ttl=64 time=0.038 ms
8980 bytes from 192.168.1.1: icmp_seq=3 ttl=64 time=0.041 ms
--- 192.168.1.1 ping statistics ---
3 packets transmitted, 3 received, 0% packet loss, time 2004ms
rtt min/avg/max/mdev = 0.038/0.041/0.045/0.003 ms

ROOT CAUSE ANALYSIS:
During the scheduled network switch maintenance at 14:00 UTC, Top-of-Rack switch Leaf-02 had jumbo frames (MTU 9000) enabled globally on all ports. However, host k8s-worker-gpu-04 (192.168.1.105) retained a stale netplan configuration with MTU hardcoded to standard 1500 bytes. When ingress traffic containing unsegmented 8KB payloads arrived, the NIC driver truncated the frames and flagged buffer overruns, causing packet loss.

RESOLUTION:
1. Aligned netplan YAML on all GPU nodes to enforce mtu: 9000.
2. Verified Ansible automation playbook `roles/networking/tasks/main.yml` includes automated MTU validation checks prior to joining the cluster pool.
3. Node 192.168.1.105 rejoined Kubernetes active worker pool at 14:35 UTC. All error rates dropped to 0.00%.
