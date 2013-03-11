BUILDER_IS_EL6 = $(shell rpm --eval '%{?el6:true}%{!?el6:false}')

# Top-level Makefile
SUBDIRS ?= $(shell find . -mindepth 2 -maxdepth 2 -name Makefile | sed  -e '/.*\.old/d' -e 's/^\.\/\([^/]*\)\/.*$$/\1/')

.PHONY: all rpms docs subdirs $(SUBDIRS)

all: TARGET=all
rpms: TARGET=rpms
docs: TARGET=docs

all rpms docs subdirs: $(SUBDIRS)

cleandist:
	rm -rf dist

dist: cleandist
	mkdir dist

agent:
	# On non-EL6 builders, we'll only do an agent build
	$(BUILDER_IS_EL6) || $(MAKE) -C chroma-agent $(TARGET)
	$(BUILDER_IS_EL6) || cp -a chroma-agent/dist/* dist/

$(SUBDIRS): dist agent
	set -e; \
	if $(BUILDER_IS_EL6); then \
		$(MAKE) -C $@ $(TARGET); \
		if [ -d $@/dist/ ]; then \
			cp -a $@/dist/* dist/; \
		fi; \
	fi

repo: rpms
	$(MAKE) -C chroma-dependencies repo

bundles: repo
	$(MAKE) -C chroma-bundles

deps: repo

# build the chroma-{agent,management} subdirs before the chroma-dependencies subdir
chroma-dependencies: chroma-agent chroma-manager
chroma-bundles: chroma-dependencies
