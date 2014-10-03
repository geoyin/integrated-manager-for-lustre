#
# INTEL CONFIDENTIAL
#
# Copyright 2013-2014 Intel Corporation All Rights Reserved.
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

from ..lib import shell
from blockdevice import BlockDevice


class ExportedZfsDevice(object):
    '''
    This allows the enclosed code to read the status, attributes etc of a zfs device that is not currently imported to the machine.
    The code imports in read only mode and then exports the device whilst the enclosed code can then operate on the device as if
    it was locally active.
    '''
    def __init__(self, device_path):
        self.device_path = device_path
        self.pool_imported = False

    def __enter__(self):
        imported_pools = shell.try_run(["zpool", "list", "-H", "-o", "name"]).split('\n')

        if self.device_path not in imported_pools:
            shell.try_run(['zpool', 'import', '-f', '-o', 'readonly=on', self.device_path])
            self.pool_imported = True

    def __exit__(self, type, value, traceback):
        if self.pool_imported:
            shell.try_run(['zpool', 'export', self.device_path])
            self.pool_imported = False


class BlockDeviceZfs(BlockDevice):
    _supported_device_types = ['zfs']

    def __init__(self, device_type, device_path):
        self._zfs_properties = None

        super(BlockDeviceZfs, self).__init__(device_type, device_path)

    @property
    def filesystem_type(self):
        # We should verify the value, but for now lets just presume.
        return 'zfs'

    @property
    def uuid(self):
        '''
        This method is simpler than the method above, but doesn't yet work with exported pools, however I want to keep
        the code so have left it in place. The code works and has tests.
        :return:
        '''
        try:
            out = shell.try_run(['zfs', 'get', '-H', '-o', 'value', 'guid', self._device_path])
        except (shell.CommandExecutionError, OSError):
            try:
                with ExportedZfsDevice(self.device_path):
                    out = shell.try_run(['zfs', 'get', '-H', '-o', 'value', 'guid', self._device_path])
            except (shell.CommandExecutionError, OSError):      # Errors or zfs not found.
                out = ''

        lines = [l for l in out.split("\n") if len(l) > 0]

        if len(lines) == 1:
            return lines[0]

        raise RuntimeError("Unable to find UUID for device %s" % self._device_path)

    @property
    def preferred_fstype(self):
        return 'zfs'

    def zfs_properties(self, log=None):
        if not self._zfs_properties:
            self._zfs_properties = {}

            try:
                ls = shell.try_run(["zfs", "get", "-Hp", "-o", "property,value", "all", self._device_path])
            except (shell.CommandExecutionError, OSError):          # Errors or zfs not found.
                try:
                    with ExportedZfsDevice(self.device_path):
                        ls = shell.try_run(["zfs", "get", "-Hp", "-o", "property,value", "all", self._device_path])
                except (shell.CommandExecutionError, OSError):      # Errors or zfs not found.
                    return self._zfs_properties

            for line in ls.split("\n"):
                try:
                    key, value = line.split('\t')

                    self._zfs_properties[key] = value
                except ValueError:
                    # Be resilient to things we don't understand.
                    if log:
                        log.info("zfs get for %s returned %s which was not parsable." % (self._device_path, line))
                    pass

        return self._zfs_properties

    def mgs_targets(self, log):
        zfs_properties = self.zfs_properties(log)

        if ('lustre:fsname' in zfs_properties) and ('lustre:svname' in zfs_properties):
            return {zfs_properties['lustre:fsname']: {"name": zfs_properties['lustre:svname'][len(zfs_properties['lustre:fsname']) + 1:]}}
        else:
            return {}

    def targets(self, uuid_name_to_target, device, log):
        log.info("Searching device %s of type %s, uuid %s for a Lustre filesystem" % (device['path'], device['type'], device['uuid']))

        zfs_properties = self.zfs_properties(log)

        if ('lustre:svname' not in zfs_properties) or ('lustre:flags' not in zfs_properties):
            log.info("Device %s did not have a Lustre property values required" % device['path'])
            return self.TargetsInfo([], None)

        # For a Lustre block device, extract name and params
        # ==================================================
        name = zfs_properties['lustre:svname']
        flags = int(zfs_properties['lustre:flags'], 16)

        if  ('lustre:mgsnode' in zfs_properties):
            params = {'mgsnode': [zfs_properties['lustre:mgsnode']]}
        else:
            params = {}

        if name.find("ffff") != -1:
            log.info("Device %s reported an unregistered lustre target and so will not be reported" % device['path'])
            return self.TargetsInfo([], None)

        if flags & 0x0005 == 0x0005:
            # For combined MGS/MDT volumes, synthesise an 'MGS'
            names = ["MGS", name]
        else:
            names = [name]

        return self.TargetsInfo(names, params)
