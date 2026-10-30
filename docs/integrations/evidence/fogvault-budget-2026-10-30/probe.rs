
use datafog_fogvault_native::{Owner,core,vault};
use datafog_fogvault_migration::stage_authenticated_bundle;
use std::{error::Error,time::Instant,process::Command};
fn id()->[u8;16]{let mut x=[0;16];getrandom::fill(&mut x).unwrap();x}
fn rss()->u64{let o=Command::new("ps").args(["-o","rss=","-p",&std::process::id().to_string()]).output().unwrap();String::from_utf8(o.stdout).unwrap().trim().parse::<u64>().unwrap()*1024}
fn main()->Result<(),Box<dyn Error>>{
 let args=std::env::args().collect::<Vec<_>>();let mode=&args[1];let input=&args[2];let pending:usize=args[3].parse()?;
 let started=Instant::now();let mut points=Vec::new();points.push(serde_json::json!({"phase":"initial","rss_bytes":rss()}));
 let temp=tempfile::tempdir()?;let host=vault::Host::create_new(temp.path().join("control"))?;let path=temp.path().join("vault.sqlite");
 let limits=vault::Limits{max_payload_bytes:core::MAX_BUNDLE_BYTES,max_metadata_bytes:88,max_records:64};
 let password=b"synthetic only resource qualification password";let mut v=host.create(&path,password,limits,vault::KdfPolicy::baseline())?;let token=v.unlock(password)?;let record=id();
 let (mut owner,namespace,source_bytes)=if mode=="history"{
  let source=std::fs::read(input)?;let source_core=core::Workspace::open_bundle(&source)?;let namespace=source_core.namespace()?;let source_head=source_core.head()?;
  let candidate=stage_authenticated_bundle(&source,namespace,source_head,id())?;
  points.push(serde_json::json!({"phase":"source_and_recovery_candidate","rss_bytes":rss(),"source_bytes":source.len(),"candidate_bytes":candidate.bundle().len(),"rows":source_core.operations()?.len()}));
  let owner=Owner::import_recovered(v,token,record,&candidate,true,id())?;
  points.push(serde_json::json!({"phase":"import_with_source_core_and_candidate_retained","rss_bytes":rss()}));
  (owner,namespace,source.len())
 }else{let namespace=id();(Owner::create(v,token,record,namespace,id(),id())?,namespace,0)};
 let mut source=String::new();let mut findings=Vec::new();
 if mode=="mappings" || mode=="history" {for i in 0..if mode=="history" {1}else{input.parse::<usize>()?}{let value=format!("synthetic{i}@example.test");let start=source.len();source.push_str(&value);findings.push(core::WorkspaceFinding::new("EMAIL",&value,datafog_core::TextRange{start,end:source.len()}));}}
 let mut plans=Vec::new();for _ in 0..pending{plans.push(owner.prepare(&source,&findings,id())?);}
 let candidate_bytes=plans.first().ok_or("pending must be nonzero")?.candidate_bytes()?;
 points.push(serde_json::json!({"phase":"pending_plans","rss_bytes":rss(),"pending":pending,"candidate_bytes_each":candidate_bytes}));
 let mut first=plans.remove(0);let saved=owner.commit(&mut first,true,id())?;
 points.push(serde_json::json!({"phase":"confirmed_save_other_plans_retained","rss_bytes":rss()}));
 drop(plans);drop(first);owner.lock();drop(owner);
 points.push(serde_json::json!({"phase":"locked_and_dropped","rss_bytes":rss()}));
 let mut v=host.open_vault(&path,limits,vault::KdfPolicy::baseline())?;let token=v.unlock(password)?;let mut owner=Owner::open(v,token,record,namespace,Some(saved.head))?;
 if owner.restore(&saved.previews[0])?!=source{return Err("roundtrip mismatch".into())}
 let rows=owner.operations()?.len();let counts=owner.counts()?;
 points.push(serde_json::json!({"phase":"authenticated_reopen_and_read","rss_bytes":rss(),"rows":rows}));
 owner.lock();drop(owner);
 println!("{}",serde_json::json!({"synthetic":true,"mode":mode,"input":input,"pending":pending,"source_bytes":source_bytes,"rows_after_commit":rows,"confirmed_handoffs":counts.confirmed_handoffs,"candidate_bytes":candidate_bytes,"elapsed_ms":started.elapsed().as_secs_f64()*1000.,"phase_points":points,"scope":"Actual Rust Core + registered FogVault native SQLite/KDF in one isolated process. Includes native runtime/allocator overhead. Excludes Node/Python foreign copies and browser/Worker processes. Process peak supplied independently by OS time."}));Ok(())
}
