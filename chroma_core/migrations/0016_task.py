# -*- coding: utf-8 -*-
# Generated by Django 1.11.27 on 2020-05-12 13:34
from __future__ import unicode_literals

import chroma_core.models.task
import django.contrib.postgres.fields
import django.contrib.postgres.fields.jsonb
from django.db import migrations, models
import django.db.models.deletion
from chroma_core.migrations import forward_lustre_fid, backward_lustre_fid


class Migration(migrations.Migration):

    dependencies = [
        ("chroma_core", "0015_device"),
    ]

    operations = [
        migrations.RunSQL(sql=forward_lustre_fid, reverse_sql=backward_lustre_fid),
        migrations.CreateModel(
            name="CreateTaskJob",
            fields=[
                (
                    "job_ptr",
                    models.OneToOneField(
                        auto_created=True,
                        on_delete=django.db.models.deletion.CASCADE,
                        parent_link=True,
                        primary_key=True,
                        serialize=False,
                        to="chroma_core.Job",
                    ),
                ),
            ],
            options={
                "ordering": ["id"],
            },
            bases=("chroma_core.job",),
        ),
        migrations.CreateModel(
            name="FidTaskError",
            fields=[
                ("id", models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                ("fid", chroma_core.models.task.LustreFidField()),
                ("data", django.contrib.postgres.fields.jsonb.JSONField(default={})),
                ("errno", models.PositiveSmallIntegerField(default=0)),
            ],
        ),
        migrations.CreateModel(
            name="FidTaskQueue",
            fields=[
                ("id", models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                ("fid", chroma_core.models.task.LustreFidField()),
                ("data", django.contrib.postgres.fields.jsonb.JSONField(default={})),
            ],
        ),
        migrations.CreateModel(
            name="RemoveTaskJob",
            fields=[
                (
                    "job_ptr",
                    models.OneToOneField(
                        auto_created=True,
                        on_delete=django.db.models.deletion.CASCADE,
                        parent_link=True,
                        primary_key=True,
                        serialize=False,
                        to="chroma_core.Job",
                    ),
                ),
            ],
            options={
                "ordering": ["id"],
            },
            bases=("chroma_core.job",),
        ),
        migrations.CreateModel(
            name="Task",
            fields=[
                ("id", models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                ("name", models.CharField(max_length=128)),
                ("start", models.DateTimeField()),
                ("finish", models.DateTimeField(blank=True, null=True)),
                ("state", models.CharField(max_length=16)),
                ("fids_total", models.BigIntegerField(default=0)),
                ("fids_completed", models.BigIntegerField(default=0)),
                ("fids_failed", models.BigIntegerField(default=0)),
                ("data_transfered", models.BigIntegerField(default=0)),
                ("single_runner", models.BooleanField(default=False)),
                ("keep_failed", models.BooleanField(default=True)),
                ("actions", django.contrib.postgres.fields.ArrayField(base_field=models.TextField(), size=None)),
                ("args", django.contrib.postgres.fields.jsonb.JSONField(default={})),
                (
                    "filesystem",
                    models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, to="chroma_core.ManagedFilesystem"),
                ),
                (
                    "running_on",
                    models.ForeignKey(
                        blank=True,
                        null=True,
                        on_delete=django.db.models.deletion.SET_NULL,
                        to="chroma_core.ManagedHost",
                    ),
                ),
            ],
        ),
        migrations.AddField(
            model_name="removetaskjob",
            name="task",
            field=models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, to="chroma_core.Task"),
        ),
        migrations.AddField(
            model_name="fidtaskqueue",
            name="task",
            field=models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, to="chroma_core.Task"),
        ),
        migrations.AddField(
            model_name="fidtaskerror",
            name="task",
            field=models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, to="chroma_core.Task"),
        ),
        migrations.AddField(
            model_name="createtaskjob",
            name="task",
            field=models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, to="chroma_core.Task"),
        ),
        migrations.AlterUniqueTogether(
            name="task",
            unique_together=set([("name",)]),
        ),
    ]
