package org.corpus_tools.graphannis.info;

import org.bytedeco.javacpp.annotation.Namespace;
import org.bytedeco.javacpp.annotation.Platform;
import org.bytedeco.javacpp.annotation.Properties;
import org.bytedeco.javacpp.tools.Info;
import org.bytedeco.javacpp.tools.InfoMap;
import org.bytedeco.javacpp.tools.InfoMapper;

@Namespace("annis::api")
@Properties(target="org.corpus_tools.graphannis.API",
    value={@Platform(
        include={
          "annis/api/search.h", 
          "annis/api/admin.h"}, 
        link={"re2" ,"boost_system", "boost_filesystem", "humblelogging", "annis"}
        ),
        @Platform(value = "windows",
		  link={"re2", "boost_system-vc140-mt-1_61", "boost_filesystem-vc140-mt-1_61", "humblelogging", "annis"}
		)})
public class AnnisApiInfo implements InfoMapper
{

  @Override
  public void map(InfoMap infoMap)
  {
	  infoMap.put(new Info("std::vector<std::string>").pointerTypes("StringVector").define());
    infoMap.put(new Info("annis::Init").skip());
    infoMap.put(new Info("std::uint32_t").valueTypes("long"));
    infoMap.put(new Info("uint32_t").valueTypes("long"));
    infoMap.put(new Info("hash<annis::Annotation>").skip());
  }

}
