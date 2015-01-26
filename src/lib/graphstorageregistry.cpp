#include "graphstorageregistry.h"

#include "edgedb/coverageedb.h"
#include "edgedb/fallbackedgedb.h"
#include "edgedb/linearedgedb.h"
#include "edgedb/prepostorderstorage.h"

using namespace annis;

GraphStorageRegistry::GraphStorageRegistry()
{
  // set default values
  setImplementation("coverage", ComponentType::COVERAGE);
  setImplementation("linear", ComponentType::ORDERING);
  setImplementation("prepostorder", ComponentType::DOMINANCE);

}

GraphStorageRegistry::~GraphStorageRegistry()
{

}

std::string annis::GraphStorageRegistry::getName(const annis::EdgeDB *db)
{
  if(dynamic_cast<const CoverageEdgeDB*>(db) != nullptr)
  {
    return coverage;
  }
  else if(dynamic_cast<const LinearEdgeDB*>(db) != nullptr)
  {
    return linear;
  }
  else if(dynamic_cast<const PrePostOrderStorage*>(db) != nullptr)
  {
    return prepostorder;
  }
  else if(dynamic_cast<const FallbackEdgeDB*>(db) != nullptr)
  {
    return fallback;
  }
  return "";
}

EdgeDB *GraphStorageRegistry::createEdgeDB(std::string name, StringStorage& strings, const Component& component)
{
  if(name == coverage)
  {
    return new CoverageEdgeDB(strings, component);
  }
  else if(name == linear)
  {
    return new LinearEdgeDB(strings, component);
  }
  else if(name == prepostorder)
  {
    return new PrePostOrderStorage(strings, component);
  }
  else if(name == fallback)
  {
    return new FallbackEdgeDB(strings, component);
  }

  return nullptr;
}

std::string GraphStorageRegistry::getOptimizedImpl(const Component &component)
{
  std::string result = fallback;
  // try to find a fully matching entry
  auto it = componentToImpl.find(component);
  if(it != componentToImpl.end())
  {
    result = it->second;
  }
  else
  {
    // try without the name
    Component withoutName = Init::initComponent(component.type, component.layer, "");
    it = componentToImpl.find(withoutName);
    if(it != componentToImpl.end())
    {
      result = it->second;
    }
    else
    {
      // try only the component type
      Component onlyType = Init::initComponent(component.type, "", "");
      it = componentToImpl.find(onlyType);
      if(it != componentToImpl.end())
      {
        result = it->second;
      }
    }
  }

  return result;
}

EdgeDB *GraphStorageRegistry::createEdgeDB(StringStorage &strings, const Component &component)
{
  std::string implName = getOptimizedImpl(component);
  return createEdgeDB(implName, strings, component);
}

void GraphStorageRegistry::setImplementation(std::string implName, ComponentType type)
{
  Component c = Init::initComponent(type, "", "");
  componentToImpl[c] = implName;
}

void GraphStorageRegistry::setImplementation(std::string implName, ComponentType type, std::string layer)
{
  Component c = Init::initComponent(type, layer, "");
  componentToImpl[c] = implName;
}

void GraphStorageRegistry::setImplementation(std::string implName, ComponentType type, std::string layer, std::string name)
{
  Component c = Init::initComponent(type, layer, name);
  componentToImpl[c] = implName;
}
