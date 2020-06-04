// Copyright (c) 2020 DDN. All rights reserved.
// Use of this source code is governed by a MIT-style
// license that can be found in the LICENSE file.

use async_trait::async_trait;
use future::Either;
use futures::{future, Future, TryFutureExt};
use iml_orm::{
    sfa::{SfaClassError, SfaDiskDrive, SfaEnclosure, SfaJob, SfaPowerSupply, SfaStorageSystem},
    tokio_diesel::{AsyncError, AsyncRunQueryDsl as _},
    AsyncRunQueryDslPostgres, Changeable, DbPool, Executable, GetChanges as _, Identifiable,
    Upserts,
};
use iml_tracing::tracing;
use std::{convert::TryInto as _, time::Duration};
use thiserror::Error;
use tokio::time;
use url::Url;
use wbem_client::{resp::Instance, Client, ClientExt};

#[derive(Error, Debug)]
enum ImlSfaError {
    #[error(transparent)]
    WbemClient(#[from] wbem_client::WbemClientError),
    #[error(transparent)]
    SfaClass(#[from] SfaClassError),
    #[error(transparent)]
    Async(#[from] AsyncError),
    #[error(transparent)]
    ImlOrm(#[from] iml_orm::ImlOrmError),
}

#[tokio::main]
async fn main() -> Result<(), ImlSfaError> {
    iml_tracing::init();

    let pool = iml_orm::pool()?;

    let endpoints = iml_manager_env::get_sfa_endpoints();

    let endpoints = match endpoints {
        Some(x) => x,
        None => {
            tracing::info!("No endpoints found, exiting.");

            return Ok(());
        }
    };

    let client = wbem_client::get_client(true)?;

    let mut interval = time::interval(Duration::from_secs(5));

    let mut old_drives = SfaDiskDrive::all()
        .load_async::<SfaDiskDrive>(&pool)
        .await?;

    let mut old_enclosures = SfaEnclosure::all()
        .load_async::<SfaEnclosure>(&pool)
        .await?;

    let mut old_jobs = SfaJob::all().load_async::<SfaJob>(&pool).await?;

    let mut old_power_supplies = SfaPowerSupply::all()
        .load_async::<SfaPowerSupply>(&pool)
        .await?;

    loop {
        interval.tick().await;

        let fut1 = client.fetch_sfa_enclosures(endpoints[0][0].clone());

        let fut2 = client.fetch_sfa_storage_system(endpoints[0][0].clone());

        let fut3 = client.fetch_sfa_disk_drives(endpoints[0][0].clone());

        let fut4 = client.fetch_sfa_jobs(endpoints[0][0].clone());

        let fut5 = client.fetch_sfa_power_supply(endpoints[0][0].clone());

        let (new_enclosures, x, new_drives, new_jobs, new_power_supplies) =
            future::try_join5(fut1, fut2, fut3, fut4, fut5).await?;

        tracing::trace!("SfaStorageSystem {:?}", x);
        tracing::trace!("SfaEnclosures {:?}", new_enclosures);
        tracing::trace!("SfaDiskDrives {:?}", new_drives);
        tracing::trace!("SfaJobs {:?}", new_jobs);
        tracing::trace!("SfaPowerSupply {:?}", new_power_supplies);

        let (enclosure_upsert, enclosure_remove) = diff_items(
            &new_enclosures,
            &old_enclosures,
            &pool,
            SfaEnclosure::batch_upsert,
            SfaEnclosure::batch_delete,
        );

        let (drive_upsert, drive_remove) = diff_items(
            &new_drives,
            &old_drives,
            &pool,
            SfaDiskDrive::batch_upsert,
            SfaDiskDrive::batch_delete,
        );

        let (job_upsert, job_remove) = diff_items(
            &new_jobs,
            &old_jobs,
            &pool,
            SfaJob::batch_upsert,
            SfaJob::batch_delete,
        );

        let (power_supply_upsert, power_supply_remove) = diff_items(
            &new_power_supplies,
            &old_power_supplies,
            &pool,
            SfaPowerSupply::batch_upsert,
            SfaPowerSupply::batch_delete,
        );

        SfaStorageSystem::upsert(x).execute_async(&pool).await?;

        enclosure_upsert.await?;

        power_supply_upsert.await?;

        power_supply_remove.await?;

        drive_upsert.await?;

        job_upsert.await?;

        job_remove.await?;

        drive_remove.await?;

        enclosure_remove.await?;

        old_drives = new_drives;

        old_enclosures = new_enclosures;

        old_jobs = new_jobs;

        old_power_supplies = new_power_supplies;
    }
}

type UpsertFn<'a, T, R> = fn(Upserts<&'a T>) -> R;
type DeleteFn<'a, T, R> = fn(Vec<&'a T>) -> R;

fn diff_items<
    'a,
    T: Identifiable + Changeable,
    R1: AsyncRunQueryDslPostgres + Executable + 'a,
    R2: AsyncRunQueryDslPostgres + Executable + 'a,
>(
    new: &'a Vec<T>,
    old: &'a Vec<T>,
    pool: &'a DbPool,
    upsert_fn: UpsertFn<'a, T, R1>,
    delete_fn: DeleteFn<'a, T, R2>,
) -> (
    impl Future<Output = Result<(), ImlSfaError>> + 'a,
    impl Future<Output = Result<(), ImlSfaError>> + 'a,
) {
    let (upsert, remove) = new.get_changes(&old);

    let upserts = if let Some(upsert) = upsert {
        tracing::debug!("{} changed Enclosures, performing Upsert", upsert.0.len());
        Either::Left(
            upsert_fn(upsert)
                .execute_async(pool)
                .map_ok(drop)
                .err_into(),
        )
    } else {
        Either::Right(future::ok(()))
    };

    let removals = if let Some(remove) = remove {
        tracing::debug!("{} removed Items, performing Deletion", remove.0.len());

        Either::Left(
            delete_fn(remove.0)
                .execute_async(pool)
                .map_ok(drop)
                .err_into(),
        )
    } else {
        Either::Right(future::ok(()))
    };

    (upserts, removals)
}

#[async_trait(?Send)]
trait SfaClassExt: ClientExt {
    async fn fetch_sfa_storage_system(&self, url: Url) -> Result<SfaStorageSystem, ImlSfaError>;
    async fn fetch_sfa_enclosures(&self, url: Url) -> Result<Vec<SfaEnclosure>, ImlSfaError>;
    async fn fetch_sfa_disk_drives(&self, url: Url) -> Result<Vec<SfaDiskDrive>, ImlSfaError>;
    async fn fetch_sfa_jobs(&self, url: Url) -> Result<Vec<SfaJob>, ImlSfaError>;
    async fn fetch_sfa_power_supply(&self, url: Url) -> Result<Vec<SfaPowerSupply>, ImlSfaError>;
}

#[async_trait(?Send)]
impl SfaClassExt for Client {
    async fn fetch_sfa_storage_system(&self, url: Url) -> Result<SfaStorageSystem, ImlSfaError> {
        let x = self
            .get_instance(url, "root/ddn", "DDN_SFAStorageSystem")
            .await?;

        let x: SfaStorageSystem = x.try_into()?;

        Ok(x)
    }
    async fn fetch_sfa_enclosures(&self, url: Url) -> Result<Vec<SfaEnclosure>, ImlSfaError> {
        let x = self.fetch_sfa_storage_system(url.clone());

        let ys = self
            .enumerate_instances(url, "root/ddn", "DDN_SFAEnclosure")
            .err_into();

        let (x, ys) = future::try_join(x, ys).await?;

        let ys = Vec::<Instance>::from(ys)
            .into_iter()
            .map(|y| (x.uuid.clone(), y).try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ys)
    }
    async fn fetch_sfa_disk_drives(&self, url: Url) -> Result<Vec<SfaDiskDrive>, ImlSfaError> {
        let x = self.fetch_sfa_storage_system(url.clone());

        let ys = self
            .enumerate_instances(url, "root/ddn", "DDN_SFADiskDrive")
            .err_into();

        let (x, ys) = future::try_join(x, ys).await?;

        let ys = Vec::<Instance>::from(ys)
            .into_iter()
            .map(|y| (x.uuid.clone(), y).try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ys)
    }
    async fn fetch_sfa_jobs(&self, url: Url) -> Result<Vec<SfaJob>, ImlSfaError> {
        let x = self.fetch_sfa_storage_system(url.clone());

        let ys = self
            .enumerate_instances(url, "root/ddn", "DDN_SFAJob")
            .err_into();

        let (x, ys) = future::try_join(x, ys).await?;

        let ys = Vec::<Instance>::from(ys)
            .into_iter()
            .map(|y| (x.uuid.clone(), y).try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ys)
    }
    async fn fetch_sfa_power_supply(&self, url: Url) -> Result<Vec<SfaPowerSupply>, ImlSfaError> {
        let x = self.fetch_sfa_storage_system(url.clone());

        let ys = self
            .enumerate_instances(url, "root/ddn", "DDN_SFAPowerSupply")
            .err_into();

        let (x, ys) = future::try_join(x, ys).await?;

        let ys = Vec::<Instance>::from(ys)
            .into_iter()
            .map(|y| (x.uuid.clone(), y).try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ys)
    }
}
