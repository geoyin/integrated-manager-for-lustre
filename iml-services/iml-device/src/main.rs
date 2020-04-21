// Copyright (c) 2020 DDN. All rights reserved.
// Use of this source code is governed by a MIT-style
// license that can be found in the LICENSE file.

use chrono::prelude::*;
use device_types::devices::{
    Device, LogicalVolume, MdRaid, Mpath, Partition, Root, ScsiDevice, VolumeGroup, Zpool,
};
use diesel::{self, pg::upsert::excluded, prelude::*};
use futures::{lock::Mutex, TryFutureExt, TryStreamExt};
use im::HashSet;
use iml_device::{
    linux_plugin_transforms::{
        build_device_lookup, devtree2linuxoutput, get_shared_pools, populate_zpool, update_vgs,
        LinuxPluginData,
    },
    ImlDeviceError,
};
use iml_orm::{
    models::{ChromaCoreDevice, NewChromaCoreDevice},
    schema::chroma_core_device::{devices, fqdn, table},
    tokio_diesel::*,
    DbPool,
};
use iml_service_queue::service_queue::consume_data;
use iml_wire_types::Fqdn;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Write,
    sync::Arc,
};
use warp::Filter;

type Cache = Arc<Mutex<HashMap<Fqdn, Device>>>;

async fn create_cache(pool: &DbPool) -> Result<Cache, ImlDeviceError> {
    let data: HashMap<Fqdn, Device> = table
        .load_async(&pool)
        .await?
        .into_iter()
        .map(
            |x: ChromaCoreDevice| -> Result<(Fqdn, Device), ImlDeviceError> {
                let d = serde_json::from_value(x.devices)?;

                Ok((Fqdn(x.fqdn), d))
            },
        )
        .collect::<Result<_, _>>()?;

    Ok(Arc::new(Mutex::new(data)))
}

#[tokio::main]
async fn main() -> Result<(), ImlDeviceError> {
    iml_tracing::init();

    let addr = iml_manager_env::get_device_aggregator_addr();

    let pool = iml_orm::pool()?;

    let cache = create_cache(&pool).await?;
    let cache2 = Arc::clone(&cache);
    let cache = warp::any().map(move || Arc::clone(&cache));

    let get = warp::get().and(cache).and_then(|cache: Cache| {
        async move {
            let cache = cache.lock().await;

            let mut xs: BTreeMap<&Fqdn, _> = cache
                .iter()
                .map(|(k, v)| {
                    let mut out = LinuxPluginData::default();

                    devtree2linuxoutput(&v, None, &mut out);

                    (k, out)
                })
                .collect();

            let (path_index, cluster_pools): (HashMap<&Fqdn, _>, HashMap<&Fqdn, _>) = cache
                .iter()
                .map(|(k, v)| {
                    let mut path_to_mm = BTreeMap::new();
                    let mut pools = BTreeMap::new();

                    build_device_lookup(v, &mut path_to_mm, &mut pools);

                    ((k, path_to_mm), (k, pools))
                })
                .unzip();

            for (&h, x) in xs.iter_mut() {
                let path_to_mm = &path_index[h];
                let shared_pools = get_shared_pools(&h, path_to_mm, &cluster_pools);

                for (a, b) in shared_pools {
                    populate_zpool(a, b, x);
                }
            }

            let xs: BTreeMap<&Fqdn, LinuxPluginData> = update_vgs(xs, &path_index);

            Ok::<_, ImlDeviceError>(warp::reply::json(&xs))
        }
        .map_err(warp::reject::custom)
    });

    tracing::info!("Server starting");

    let server = warp::serve(get.with(warp::log("devices"))).run(addr);

    tokio::spawn(server);

    let mut s = consume_data::<Device>("rust_agent_device_rx");

    let mut i = 0usize;
    while let Some((f, d)) = s.try_next().await? {
        let begin: DateTime<Local> = Local::now();
        tracing::info!("Iteration {}: begin: {}", i, begin);

        let mut cache = cache2.lock().await;

        assert!(
            match d {
                Device::Root(_) => true,
                _ => false,
            },
            "The top device has to be Root"
        );

        println!("Host: {}", f);

        let mut ff = File::create(format!("/tmp/device{}", i)).unwrap();
        ff.write_all(serde_json::to_string_pretty(&d).unwrap().as_bytes())
            .unwrap();
        let mut parents = vec![];
        let mut ff = File::create(format!("/tmp/parents{}", i)).unwrap();
        ff.write_all(serde_json::to_string_pretty(&parents).unwrap().as_bytes())
            .unwrap();

        collect_virtual_device_parents(&d, 0, None, &mut parents);

        let other_devices = table
            .filter(fqdn.ne(f.to_string()))
            .load_async::<ChromaCoreDevice>(&pool)
            .await
            .expect("Error getting devices from other hosts");

        for (j, ccd) in other_devices.into_iter().enumerate() {
            let f = ccd.fqdn;
            let d = ccd.devices;

            let mut d: Device =
                serde_json::from_value(d).expect("Couldn't read Device from JSON from DB");

            insert_virtual_devices(&mut d, &*parents);

            let mut ff = File::create(format!("/tmp/otherdevice{}-{}", i, j)).unwrap();
            ff.write_all(serde_json::to_string_pretty(&d).unwrap().as_bytes())
                .unwrap();

            let device_to_insert = NewChromaCoreDevice {
                fqdn: f.to_string(),
                device: serde_json::to_value(d).expect("Could not convert incoming Devices to JSON."),
            };
    
            let new_device = diesel::insert_into(table)
                .values(device_to_insert)
                .on_conflict(fqdn)
                .do_update()
                .set(device.eq(excluded(device)))
                .get_result_async::<ChromaCoreDevice>(&pool)
                .await
                .expect("Error saving new device");
    
            tracing::info!("Inserted other device from host {}", new_device.fqdn);
            tracing::trace!("Inserted other device {:?}", new_device);
        }

        cache.insert(f.clone(), d.clone());

        let device_to_insert = NewChromaCoreDevice {
            fqdn: f.to_string(),
            devices: serde_json::to_value(d).expect("Could not convert incoming Devices to JSON."),
        };

        let new_device = diesel::insert_into(table)
            .values(device_to_insert)
            .on_conflict(fqdn)
            .do_update()
            .set(devices.eq(excluded(devices)))
            .get_result_async::<ChromaCoreDevice>(&pool)
            .await
            .expect("Error saving new device");

        tracing::info!("Inserted device from host {}", new_device.fqdn);
        tracing::trace!("Inserted device {:?}", new_device);

        let end: DateTime<Local> = Local::now();

        tracing::info!(
            "Iteration {}: end: {}, duration: {}",
            i,
            end,
            end - begin
        );

        i = i.wrapping_add(1);
    }

    Ok(())
}

fn is_virtual(d: &Device) -> bool {
    match d {
        Device::Dataset(_)
        | Device::LogicalVolume(_)
        | Device::MdRaid(_)
        | Device::VolumeGroup(_)
        | Device::Zpool(_) => true,
        _ => false,
    }
}

fn collect_virtual_device_parents<'d>(
    d: &'d Device,
    level: usize,
    parent: Option<&'d Device>,
    mut parents: &mut Vec<&'d Device>,
) {
    if is_virtual(d) {
        parents.push(
            parent.expect("Tried to push to parents the parent of the Root, which doesn't exist"),
        );
    } else {
        match d {
            Device::Root(dd) => {
                for c in dd.children.iter() {
                    collect_virtual_device_parents(c, level + 1, Some(d), &mut parents);
                }
            }
            Device::ScsiDevice(dd) => {
                for c in dd.children.iter() {
                    collect_virtual_device_parents(c, level + 1, Some(d), &mut parents);
                }
            }
            Device::Partition(dd) => {
                for c in dd.children.iter() {
                    collect_virtual_device_parents(c, level + 1, Some(d), &mut parents);
                }
            }
            Device::Mpath(dd) => {
                for c in dd.children.iter() {
                    collect_virtual_device_parents(c, level + 1, Some(d), &mut parents);
                }
            }
            _ => unreachable!(),
        };
    }
}

fn _walk(d: &Device) {
    match d {
        Device::Root(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::ScsiDevice(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::Partition(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::MdRaid(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::Mpath(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::VolumeGroup(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::LogicalVolume(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::Zpool(d) => {
            for c in &d.children {
                _walk(c);
            }
        }
        Device::Dataset(_) => {}
    }
}

fn insert<'a>(d: &'a mut Device, to_insert: &'a Device) {
    if compare_without_children(d, to_insert) {
        *d = to_insert.clone();
    } else {
        match d {
            Device::Root(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::ScsiDevice(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::Partition(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::MdRaid(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::Mpath(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::VolumeGroup(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::LogicalVolume(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::Zpool(d) => {
                for mut c in d.children.iter_mut() {
                    insert(&mut c, to_insert);
                }
            }
            Device::Dataset(_) => {}
        }
    }
}

fn without_children(d: &Device) -> Device {
    match d {
        Device::Root(d) => Device::Root(Root {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::ScsiDevice(d) => Device::ScsiDevice(ScsiDevice {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::Partition(d) => Device::Partition(Partition {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::MdRaid(d) => Device::MdRaid(MdRaid {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::Mpath(d) => Device::Mpath(Mpath {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::VolumeGroup(d) => Device::VolumeGroup(VolumeGroup {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::LogicalVolume(d) => Device::LogicalVolume(LogicalVolume {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::Zpool(d) => Device::Zpool(Zpool {
            children: HashSet::new(),
            ..d.clone()
        }),
        Device::Dataset(d) => Device::Dataset(d.clone()),
    }
}

fn compare_without_children(a: &Device, b: &Device) -> bool {
    without_children(a) == without_children(b)
}

fn insert_virtual_devices(d: &mut Device, parents: &[&Device]) {
    for p in parents {
        insert(d, &p);
    }
}
