Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    1
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
BuildRequires:  cmake
BuildRequires:  pkgconfig(capi-appfw-service-application)
BuildRequires:  pkgconfig(dlog)
BuildRequires:  pkgconfig(libcurl)
BuildRequires:  pkgconfig(gtest)
BuildRequires:  pkgconfig(gmock)
# C++ toolchain is generally available in Tizen

%description
TizenClaw Native Agent running as a System Service Application, utilizing LXC for skills execution.

%prep
%setup -q -n %{name}-%{version}
cp packaging/tizenclaw.manifest .

%build
export CFLAGS="$CFLAGS -Wall"
export CXXFLAGS="$CXXFLAGS -Wall"
export LDFLAGS="$LDFLAGS -Wl,--as-needed"

mkdir -p build
cd build
cmake .. \
    -DCMAKE_INSTALL_PREFIX=/usr
make %{?_smp_mflags}

%check
cd build
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:%{buildroot}/usr/lib
ctest -V

%install
rm -rf %{buildroot}
cd build
%make_install

# Tizen apps structure
mkdir -p %{buildroot}/usr/apps/org.tizen.tizenclaw/bin
mkdir -p %{buildroot}/usr/apps/org.tizen.tizenclaw/res
mkdir -p %{buildroot}/usr/apps/org.tizen.tizenclaw/data/skills
mkdir -p %{buildroot}/usr/apps/org.tizen.tizenclaw/shared/res

%files
%manifest tizenclaw.manifest
%defattr(-,root,root,-)
/usr/apps/org.tizen.tizenclaw/bin/tizenclaw
/usr/apps/org.tizen.tizenclaw/bin/tizenclaw-unittests
/usr/apps/org.tizen.tizenclaw/tizen-manifest.xml
/usr/apps/org.tizen.tizenclaw/data/rootfs.tar.gz
/usr/apps/org.tizen.tizenclaw/data/skills/
%dir /usr/apps/org.tizen.tizenclaw/shared/res
%dir /usr/apps/org.tizen.tizenclaw/bin
%dir /usr/apps/org.tizen.tizenclaw/res
%dir /usr/apps/org.tizen.tizenclaw/data
%dir /usr/apps/org.tizen.tizenclaw/shared
%dir /usr/apps/org.tizen.tizenclaw/
