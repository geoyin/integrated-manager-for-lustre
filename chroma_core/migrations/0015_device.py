# -*- coding: utf-8 -*-
# Generated by Django 1.11.23 on 2020-04-15 13:20
from __future__ import unicode_literals

import django.contrib.postgres.fields.jsonb
from django.db import migrations, models


class Migration(migrations.Migration):

    dependencies = [
        ("chroma_core", "0014_multipletimesyncalert_notimesyncalert_timeoutofsyncalert"),
    ]

    operations = [
        migrations.CreateModel(
            name="Device",
            fields=[
                ("id", models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                ("fqdn", models.CharField(max_length=255, unique=True)),
                ("devices", django.contrib.postgres.fields.jsonb.JSONField()),
            ],
        ),
    ]
