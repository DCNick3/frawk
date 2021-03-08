//! TODO: come back to the 4 test cases that couldn't be generated automatically
//! TODO: look for tests doing file io.
//! TODO: get a better way for
use assert_cmd::Command;
use std::fs::{read_to_string, File};
use std::io::Write;
use tempfile::tempdir;

#[cfg(feature = "llvm_backend")]
const BACKEND_ARGS: &'static [&'static str] = &["-binterp", "-bllvm", "-bcranelift"];
#[cfg(not(feature = "llvm_backend"))]
const BACKEND_ARGS: &'static [&'static str] = &["-binterp", "-bcranelift"];

const TEST: &'static str = r##"hello"##;

const TEST_DATA: &'static str = r#"/dev/rrp3:

17379	mel
16693	bwk	me
16116	ken	him	someone else
15713	srb
11895	lem
10409	scj
10252	rhm
 9853	shen
 9748	a68
 9492	sif
 9190	pjw
 8912	nls
 8895	dmr
 8491	cda
 8372	bs
 8252	llc
 7450	mb
 7360	ava
 7273	jrv
 7080	bin
 7063	greg
 6567	dict
 6462	lck
 6291	rje
 6211	lwf
 5671	dave
 5373	jhc
 5220	agf
 5167	doug
 5007	valerie
 3963	jca
 3895	bbs
 3796	moh
 3481	xchar
 3200	tbl
 2845	s
 2774	tgs
 2641	met
 2566	jck
 2511	port
 2479	sue
 2127	root
 1989	bsb
 1989	jeg
 1933	eag
 1801	pdj
 1590	tpc
 1385	cvw
 1370	rwm
 1316	avg
 1205	eg
 1194	jam
 1153	dl
 1150	lgm
 1031	cmb
 1018	jwr
  950	gdb
  931	marc
  898	usg
  865	ggr
  822	daemon
  803	mihalis
  700	honey
  624	tad
  559	acs
  541	uucp
  523	raf
  495	adh
  456	kec
  414	craig
  386	donmac
  375	jj
  348	ravi
  344	drw
  327	stars
  288	mrg
  272	jcb
  263	ralph
  253	tom
  251	sjb
  248	haight
  224	sharon
  222	chuck
  213	dsj
  201	bill
  184	god
  176	sys
  166	meh
  163	jon
  144	dan
  143	fox
  123	dale
  116	kab
   95	buz
   80	asc
   79	jas
   79	trt
   64	wsb
   62	dwh
   56	ktf
   54	lr
   47	dlc
   45	dls
   45	jwf
   44	mash
   43	ars
   43	vgl
   37	jfo
   32	rab
   31	pd
   29	jns
   25	spm
   22	rob
   15	egb
   10	hm
   10	mhb
    6	aed
    6	cpb
    5	evp
    4	ber
    4	men
    4	mitch
    3	ast
    3	jfr
    3	lax
    3	nel
    2	blue
    2	jfk
    2	njas
    1	122sec
    1	ddwar
    1	gopi
    1	jk
    1	learn
    1	low
    1	nac
    1	sidor
1root:EMpNB8Zp56:0:0:Super-User,,,,,,,:/:/bin/sh
2roottcsh:*:0:0:Super-User running tcsh [cbm]:/:/bin/tcsh
3sysadm:*:0:0:System V Administration:/usr/admin:/bin/sh
4diag:*:0:996:Hardware Diagnostics:/usr/diags:/bin/csh
5daemon:*:1:1:daemons:/:/bin/sh
6bin:*:2:2:System Tools Owner:/bin:/dev/null
7nuucp:BJnuQbAo:6:10:UUCP.Admin:/usr/spool/uucppublic:/usr/lib/uucp/uucico
8uucp:*:3:5:UUCP.Admin:/usr/lib/uucp:
9sys:*:4:0:System Activity Owner:/usr/adm:/bin/sh
10adm:*:5:3:Accounting Files Owner:/usr/adm:/bin/sh
11lp:*:9:9:Print Spooler Owner:/var/spool/lp:/bin/sh
12auditor:*:11:0:Audit Activity Owner:/auditor:/bin/sh
13dbadmin:*:12:0:Security Database Owner:/dbadmin:/bin/sh
14bootes:dcon:50:1:Tom Killian (DO NOT REMOVE):/tmp:
15cdjuke:dcon:51:1:Tom Killian (DO NOT REMOVE):/tmp:
16rfindd:*:66:1:Rfind Daemon and Fsdump:/var/rfindd:/bin/sh
17EZsetup:*:992:998:System Setup:/var/sysadmdesktop/EZsetup:/bin/csh
18demos:*:993:997:Demonstration User:/usr/demos:/bin/csh
19tutor:*:994:997:Tutorial User:/usr/tutor:/bin/csh
20tour:*:995:997:IRIS Space Tour:/usr/people/tour:/bin/csh
21guest:nfP4/Wpvio/Rw:998:998:Guest Account:/usr/people/guest:/bin/csh
224Dgifts:0nWRTZsOMt.:999:998:4Dgifts Account:/usr/people/4Dgifts:/bin/csh
23nobody:*:60001:60001:SVR4 nobody uid:/dev/null:/dev/null
24noaccess:*:60002:60002:uid no access:/dev/null:/dev/null
25nobody:*:-2:-2:original nobody uid:/dev/null:/dev/null
26rje:*:8:8:RJE Owner:/usr/spool/rje:
27changes:*:11:11:system change log:/:
28dist:sorry:9999:4:file distributions:/v/adm/dist:/v/bin/sh
29man:*:99:995:On-line Manual Owner:/:
30phoneca:*:991:991:phone call log [tom]:/v/adm/log:/v/bin/sh
1r oot EMpNB8Zp56 0 0 Super-User,,,,,,, / /bin/sh
2r oottcsh * 0 0 Super-User running tcsh [cbm] / /bin/tcsh
3s ysadm * 0 0 System V Administration /usr/admin /bin/sh
4d iag * 0 996 Hardware Diagnostics /usr/diags /bin/csh
5d aemon * 1 1 daemons / /bin/sh
6b in * 2 2 System Tools Owner /bin /dev/null
7n uucp BJnuQbAo 6 10 UUCP.Admin /usr/spool/uucppublic /usr/lib/uucp/uucico
8u ucp * 3 5 UUCP.Admin /usr/lib/uucp 
9s ys * 4 0 System Activity Owner /usr/adm /bin/sh
10 adm * 5 3 Accounting Files Owner /usr/adm /bin/sh
11 lp * 9 9 Print Spooler Owner /var/spool/lp /bin/sh
12 auditor * 11 0 Audit Activity Owner /auditor /bin/sh
13 dbadmin * 12 0 Security Database Owner /dbadmin /bin/sh
14 bootes dcon 50 1 Tom Killian (DO NOT REMOVE) /tmp 
15 cdjuke dcon 51 1 Tom Killian (DO NOT REMOVE) /tmp 
16 rfindd * 66 1 Rfind Daemon and Fsdump /var/rfindd /bin/sh
17 EZsetup * 992 998 System Setup /var/sysadmdesktop/EZsetup /bin/csh
18 demos * 993 997 Demonstration User /usr/demos /bin/csh
19 tutor * 994 997 Tutorial User /usr/tutor /bin/csh
20 tour * 995 997 IRIS Space Tour /usr/people/tour /bin/csh
21 guest nfP4/Wpvio/Rw 998 998 Guest Account /usr/people/guest /bin/csh
22 4Dgifts 0nWRTZsOMt. 999 998 4Dgifts Account /usr/people/4Dgifts /bin/csh
23 nobody * 60001 60001 SVR4 nobody uid /dev/null /dev/null
24 noaccess * 60002 60002 uid no access /dev/null /dev/null
25 nobody * -2 -2 original nobody uid /dev/null /dev/null
26 rje * 8 8 RJE Owner /usr/spool/rje 
27 changes * 11 11 system change log / 
28 dist sorry 9999 4 file distributions /v/adm/dist /v/bin/sh
29 man * 99 995 On-line Manual Owner / 
30 phoneca * 991 991 phone call log [tom] /v/adm/log /v/bin/sh
"#;
