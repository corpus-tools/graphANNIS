#pragma once

#include <string>
#include <google/btree_map.h>
#include <cstdint>
#include <iostream>
#include <sstream>
#include <map>
#include <vector>
#include <list>

#include <annis/types.h>
#include <annis/stringstorage.h>
#include <annis/graphstorageregistry.h>
#include <annis/nodeannostorage.h>

namespace annis
{
  
class ReadableGraphStorage;
class WriteableGraphStorage;
  
class DB
{
  using GraphStorageIt = std::map<Component, std::shared_ptr<ReadableGraphStorage>>::const_iterator;
public:
  DB();

  bool loadRelANNIS(std::string dirPath);
  bool load(std::string dirPath);
  bool save(std::string dirPath);

  bool hasNode(nodeid_t id);
  
  inline std::string getNodeName(const nodeid_t &id) const
  {
    std::string result = "";

    std::pair<bool, Annotation> anno = nodeAnnos.getNodeAnnotation(id, annis_ns, annis_node_name);
    if(anno.first)
    {
      result = strings.str(anno.second.val);
    }
    return result;
  }

  inline std::string getNodeDocument(const nodeid_t &id) const
  {
    std::string result = "";

    std::pair<bool, Annotation> anno = nodeAnnos.getNodeAnnotation(id, annis_ns, "document");
    if(anno.first)
    {
      result = strings.str(anno.second.val);
    }
    return result;
  }

  inline std::string getNodeDebugName(const nodeid_t &id) const
  {
    std::stringstream ss;
    ss << getNodeDocument(id) << "/" << getNodeName(id) << "(" << id << ")";

    return ss.str();
  }


  std::vector<Component> getDirectConnected(const Edge& edge) const;
  std::vector<Component> getAllComponents() const;
  std::weak_ptr<const ReadableGraphStorage> getGraphStorage(const Component& component) const;
  std::weak_ptr<const ReadableGraphStorage> getGraphStorage(ComponentType type, const std::string& layer, const std::string& name) const;
  std::vector<std::weak_ptr<const ReadableGraphStorage>> getGraphStorage(ComponentType type, const std::string& name) const;
  std::vector<std::weak_ptr<const ReadableGraphStorage>> getGraphStorage(ComponentType type) const;

  std::vector<Annotation> getEdgeAnnotations(const Component& component,
                                             const Edge& edge);
  std::string info();

  inline std::uint32_t getNamespaceStringID() const {return annisNamespaceStringID;}
  inline std::uint32_t getNodeNameStringID() const {return annisNodeNameStringID;}
  inline std::uint32_t getEmptyStringID() const {return annisEmptyStringID;}
  inline std::uint32_t getTokStringID() const {return annisTokStringID;}

  void convertComponent(Component c, std::string impl = "");

  void optimizeAll(const std::map<Component, std::string> &manualExceptions = std::map<Component, std::string>());

  size_t estimateMemorySize();

  virtual ~DB();
public:

  StringStorage strings;
  NodeAnnoStorage nodeAnnos;

private:
  
  std::map<Component, std::shared_ptr<ReadableGraphStorage>> edgeDatabases;
  GraphStorageRegistry registry;

  std::uint32_t annisNamespaceStringID;
  std::uint32_t annisEmptyStringID;
  std::uint32_t annisTokStringID;
  std::uint32_t annisNodeNameStringID;

  bool loadRelANNISCorpusTab(std::string dirPath, std::map<std::uint32_t, std::uint32_t>& corpusIDToName,
    bool isANNIS33Format);
  bool loadRelANNISNode(std::string dirPath, std::map<std::uint32_t, std::uint32_t>& corpusIDToName,
    bool isANNIS33Format);
  bool loadRelANNISRank(const std::string& dirPath,
                        const std::map<uint32_t, std::shared_ptr<WriteableGraphStorage> > &componentToGS,
                        bool isANNIS33Format);

  bool loadEdgeAnnotation(const std::string& dirPath,
                          const std::map<uint32_t, std::shared_ptr<WriteableGraphStorage> > &pre2GS,
                          const std::map<std::uint32_t, Edge>& pre2Edge,
                          bool isANNIS33Format);

  
  void clear();
  void addDefaultStrings();

  std::shared_ptr<ReadableGraphStorage> createGSForComponent(const std::string& shortType, const std::string& layer,
                       const std::string& name);
  std::shared_ptr<ReadableGraphStorage> createGSForComponent(ComponentType ctype, const std::string& layer,
                       const std::string& name);
  std::shared_ptr<annis::WriteableGraphStorage> createWritableGraphStorage(ComponentType ctype, const std::string& layer,
                       const std::string& name);


  std::string getImplNameForPath(std::string directory);

  ComponentType componentTypeFromShortName(std::string shortType);

  std::string debugComponentString(const Component& c)
  {
    std::stringstream ss;
    ss << ComponentTypeHelper::toString(c.type) << "|" << c.layer
       << "|" << c.name;
    return ss.str();

  }
};

} // end namespace annis
