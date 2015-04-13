# -*- coding: utf-8 -*-
#
# INTEL CONFIDENTIAL
#
# Copyright 2013-2015 Intel Corporation All Rights Reserved.
#
# The source code contained or described herein and all documents related
# to the source code ("Material") are owned by Intel Corporation or its
# suppliers or licensors. Title to the Material remains with Intel Corporation
# or its suppliers and licensors. The Material contains trade secrets and
# proprietary and confidential information of Intel or its suppliers and
# licensors. The Material is protected by worldwide copyright and trade secret
# laws and treaty provisions. No part of the Material may be used, copied,
# reproduced, modified, published, uploaded, posted, transmitted, distributed,
# or disclosed in any way without Intel's prior express written permission.
#
# No license under any patent, copyright, trade secret or other intellectual
# property right is granted to or conferred upon you by disclosure or delivery
# of the Materials, either expressly, by implication, inducement, estoppel or
# otherwise. Any license under such intellectual property rights must be
# express and approved by Intel in writing.

from django.db import models

from chroma_core.models import Nid


class NetworkInterface(models.Model):
    host = models.ForeignKey('ManagedHost')

    name = models.CharField(max_length=32)
    inet4_address = models.CharField(max_length=128)
    inet4_prefix = models.IntegerField()
    corosync_configuration = models.ForeignKey('CorosyncConfiguration', null=True)
    type = models.CharField(max_length=32)          # tcp, o2ib, ... (best stick to lnet types!)
    state_up = models.BooleanField()

    def __str__(self):
        return "%s-%s" % (self.host, self.name)

    class Meta:
        app_label = 'chroma_core'
        ordering = ['id']
        unique_together = ('host', 'name')

    @property
    def lnd_types(self):
        return Nid.lnd_types_for_network_type(self.type)
